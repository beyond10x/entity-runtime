//! The production Eventlog facade under caller-owned PostgreSQL transport authority.
#![cfg(feature = "eventlog-facade")]

use std::{
    num::NonZeroU16,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use entity_core::Registry;
use entity_eventlog::{
    sync::{BridgeConfig, CallWait, ProvisionAuthority, ShutdownMode, ShutdownOutcome},
    Authority, EventlogOperationContext,
};
use entity_executor::{BatchAction, CreateRequest};
use entity_postgres::{EventlogPostgresStore, PostgresFacadeAdmission};
use entity_query::{DocumentQuery, DocumentQueryProvider};
use entity_store::{
    asynchronous::{AppendOutcome, BatchKey, Subject},
    HistoryProvider, RecordedObservation, Recording, StateProvider,
};
use eventlog_core::{BoxFuture, CaptureLimits, EventLogError};
use eventlog_postgres::{
    AuthorizedConnection, PoolOptions, PostgresConfig, PostgresConnectionAuthority,
    PostgresTransportAssurance,
};
use rustls::pki_types::pem::PemObject;
use serde_json::json;

struct VerifiedTlsAuthority {
    config: tokio_postgres::Config,
    tls: rustls::ClientConfig,
    calls: AtomicUsize,
}

impl PostgresConnectionAuthority for VerifiedTlsAuthority {
    fn assurance(&self) -> PostgresTransportAssurance {
        PostgresTransportAssurance::Verified
    }

    fn connect(
        &self,
        timeout: Duration,
    ) -> BoxFuture<'_, Result<AuthorizedConnection, EventLogError>> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let mut config = self.config.clone();
        config.connect_timeout(timeout);
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(self.tls.clone());
        Box::pin(async move {
            let (client, driver) = config.connect(tls).await.map_err(|_| {
                EventLogError::Backend(
                    "caller-owned verified PostgreSQL connection failed".to_owned(),
                )
            })?;
            Ok(AuthorizedConnection::new(client, driver))
        })
    }
}

fn runtime<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {}
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn operation_context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "postgres-facade-test".to_owned(),
        actor: "entity-postgres-test".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: time::OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: Some("postgres-facade".to_owned()),
        causation: None,
        actor: None,
    }
}

fn create(id: &str, record_id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        definition_version: 1,
        fields: json!({"title":id}),
        recording: recording(record_id),
    }
}

fn bridge() -> BridgeConfig {
    BridgeConfig {
        queue_capacity: NonZeroU16::new(8).expect("nonzero"),
    }
}

fn admission() -> PostgresFacadeAdmission {
    PostgresFacadeAdmission {
        options: PoolOptions::default(),
        database_connections: 16,
        replicas: 2,
        reserved_connections: 8,
        capture_limits: CaptureLimits {
            max_events: 512,
            max_blobs: 2_048,
            max_projection_rows: 2_048,
            max_payload_bytes: 8 * 1024 * 1024,
        },
    }
}

fn fixture() -> Option<(String, String, rustls::ClientConfig)> {
    let migration = match std::env::var("EVENTLOG_TEST_POSTGRES_MIGRATION_URL") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            assert!(
                std::env::var_os("EVENTLOG_REQUIRE_POSTGRES").is_none(),
                "required migration-role PostgreSQL fixture is missing"
            );
            eprintln!("skipped: EVENTLOG_TEST_POSTGRES_MIGRATION_URL is unset");
            return None;
        }
    };
    let application = std::env::var("EVENTLOG_TEST_HOSTED_POSTGRES_URL")
        .expect("explicit application-role PostgreSQL fixture URL");
    let ca = std::env::var("EVENTLOG_TEST_POSTGRES_CA").expect("explicit PostgreSQL fixture CA");
    let certificate =
        rustls::pki_types::CertificateDer::from_pem_file(ca).expect("test CA certificate");
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate).expect("trusted test CA");
    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Some((migration, application, tls))
}

fn authority(url: &str, tls: &rustls::ClientConfig) -> Arc<VerifiedTlsAuthority> {
    let mut config: tokio_postgres::Config = url.parse().expect("PostgreSQL fixture config");
    config.ssl_mode(tokio_postgres::config::SslMode::Require);
    Arc::new(VerifiedTlsAuthority {
        config,
        tls: tls.clone(),
        calls: AtomicUsize::new(0),
    })
}

fn config(schema: &str, prefix: &str, authority: Arc<VerifiedTlsAuthority>) -> PostgresConfig {
    PostgresConfig::caller_owned(schema, prefix, authority).expect("caller-owned config")
}

fn prepare_schema(
    migration_url: &str,
    application_url: &str,
    tls: &rustls::ClientConfig,
    schema: &str,
) {
    runtime(async {
        let mut migration_config: tokio_postgres::Config =
            migration_url.parse().expect("migration config");
        migration_config.ssl_mode(tokio_postgres::config::SslMode::Require);
        let (migration, migration_driver) = migration_config
            .connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls.clone()))
            .await
            .expect("migration connection");
        let migration_driver = tokio::spawn(migration_driver);
        let mut application_config: tokio_postgres::Config =
            application_url.parse().expect("application config");
        application_config.ssl_mode(tokio_postgres::config::SslMode::Require);
        let (application, application_driver) = application_config
            .connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls.clone()))
            .await
            .expect("application connection");
        let application_driver = tokio::spawn(application_driver);
        let migration_user: String = migration
            .query_one("SELECT current_user", &[])
            .await
            .expect("migration identity")
            .get(0);
        let application_user: String = application
            .query_one("SELECT current_user", &[])
            .await
            .expect("application identity")
            .get(0);
        let quoted_schema: String = migration
            .query_one("SELECT quote_ident($1)", &[&schema])
            .await
            .expect("quoted schema")
            .get(0);
        let quoted_application: String = migration
            .query_one("SELECT quote_ident($1)", &[&application_user])
            .await
            .expect("quoted application role")
            .get(0);
        let database: String = migration
            .query_one("SELECT current_database()", &[])
            .await
            .expect("database identity")
            .get(0);
        let quoted_database: String = migration
            .query_one("SELECT quote_ident($1)", &[&database])
            .await
            .expect("quoted database")
            .get(0);
        assert_ne!(migration_user, application_user, "roles must be separate");
        migration
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {quoted_schema} CASCADE;\
                 CREATE SCHEMA {quoted_schema};\
                 GRANT USAGE ON SCHEMA {quoted_schema} TO {quoted_application};\
                 GRANT TEMPORARY ON DATABASE {quoted_database} TO {quoted_application};\
                 ALTER DEFAULT PRIVILEGES IN SCHEMA {quoted_schema} \
                   GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {quoted_application};\
                 ALTER DEFAULT PRIVILEGES IN SCHEMA {quoted_schema} \
                   GRANT USAGE, SELECT ON SEQUENCES TO {quoted_application};"
            ))
            .await
            .expect("isolated schema and application grants");
        drop(application);
        drop(migration);
        application_driver
            .await
            .expect("application driver task")
            .expect("application driver retirement");
        migration_driver
            .await
            .expect("migration driver task")
            .expect("migration driver retirement");
    });
}

fn drop_schema(migration_url: &str, tls: &rustls::ClientConfig, schema: &str) {
    runtime(async {
        let mut config: tokio_postgres::Config = migration_url.parse().expect("migration config");
        config.ssl_mode(tokio_postgres::config::SslMode::Require);
        let (migration, driver) = config
            .connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls.clone()))
            .await
            .expect("migration connection");
        let driver = tokio::spawn(driver);
        let quoted_schema: String = migration
            .query_one("SELECT quote_ident($1)", &[&schema])
            .await
            .expect("quoted schema")
            .get(0);
        migration
            .batch_execute(&format!("DROP SCHEMA {quoted_schema} CASCADE"))
            .await
            .expect("drop isolated schema");
        drop(migration);
        driver
            .await
            .expect("migration driver task")
            .expect("migration driver retirement");
    });
}

#[test]
fn postgres_eventlog_facade_preserves_caller_owned_transport_and_complete_receipts_when_assigned() {
    let Some((migration_url, application_url, tls)) = fixture() else {
        return;
    };
    let schema = format!("entity_facade_{}", std::process::id());
    let prefix = "recorded";
    prepare_schema(&migration_url, &application_url, &tls, &schema);

    let migration_authority = authority(&migration_url, &tls);
    let application_authority = authority(&application_url, &tls);
    let mut store = EventlogPostgresStore::provision(
        registry(),
        config(&schema, prefix, migration_authority.clone()),
        config(&schema, prefix, application_authority.clone()),
        admission(),
        ProvisionAuthority {
            logical_scope: "scope-postgres-caller-owned".to_owned(),
            tenant: "tenant-postgres-caller-owned".to_owned(),
            expected_stream_identity: None,
        },
        operation_context("provision"),
        bridge(),
    )
    .expect("caller-owned PostgreSQL facade provisioned");
    let exact_authority = store.recorded().authority().clone();

    let first = store
        .recorded()
        .create(
            operation_context("create"),
            create("caller-owned", "postgres-caller-owned-create"),
            CallWait::Forever,
        )
        .expect("recorded creation");
    let retry = store
        .recorded()
        .create(
            operation_context("retry"),
            create("caller-owned", "postgres-caller-owned-create"),
            CallWait::Forever,
        )
        .expect("exact recorded retry");
    let (
        AppendOutcome::Committed {
            receipt: first_receipt,
            replayed: false,
        },
        AppendOutcome::Committed {
            receipt: retry_receipt,
            replayed: true,
        },
    ) = (first, retry)
    else {
        panic!("creation and exact retry must retain the committed receipt")
    };
    assert_eq!(retry_receipt, first_receipt);
    let actions = vec![
        BatchAction::Create(create("second", "postgres-batch-second")),
        BatchAction::Create(create("third", "postgres-batch-third")),
    ];
    let batch_key = BatchKey::Named("postgres-caller-owned-batch".to_owned());
    let first_batch = store
        .recorded()
        .execute_batch(
            operation_context("batch"),
            batch_key.clone(),
            actions.clone(),
            CallWait::Forever,
        )
        .expect("recorded atomic group");
    let retry_batch = store
        .recorded()
        .execute_batch(
            operation_context("batch-retry"),
            batch_key,
            actions,
            CallWait::Forever,
        )
        .expect("exact atomic-group retry");
    let (
        AppendOutcome::Committed {
            receipt: first_batch_receipt,
            replayed: false,
        },
        AppendOutcome::Committed {
            receipt: retry_batch_receipt,
            replayed: true,
        },
    ) = (first_batch, retry_batch)
    else {
        panic!("atomic group and retry must retain the committed receipt")
    };
    assert_eq!(retry_batch_receipt, first_batch_receipt);
    let observation = RecordedObservation {
        entity: "ticket".to_owned(),
        id: "caller-owned".to_owned(),
        revision: 1,
        envelope: recording("postgres-caller-owned-observation")
            .seal(json!({"transport":"caller-owned"}))
            .expect("observation envelope"),
    };
    store
        .recorded()
        .observe(
            operation_context("observe"),
            observation.clone(),
            CallWait::Forever,
        )
        .expect("recorded observation");
    assert_eq!(
        store
            .load("ticket", "caller-owned")
            .expect("recorded state")
            .expect("created instance")
            .revision,
        1
    );
    assert_eq!(
        store
            .query_documents(&DocumentQuery::for_entity("ticket"))
            .expect("recorded query")
            .items
            .len(),
        3
    );
    assert_eq!(
        store
            .observations("ticket", "caller-owned")
            .expect("recorded observations"),
        vec![observation]
    );
    assert_eq!(
        store
            .records("ticket", "caller-owned")
            .expect("recorded history")
            .len(),
        1
    );
    assert!(store
        .recorded()
        .lookup_record("postgres-batch-second", CallWait::Forever)
        .expect("original global record lookup")
        .is_some());
    assert_eq!(
        store.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    assert!(migration_authority.calls.load(Ordering::Acquire) >= 1);
    assert!(application_authority.calls.load(Ordering::Acquire) >= 1);

    let refused_authority = Authority {
        stream_identity: format!("{}-altered", exact_authority.stream_identity),
        ..exact_authority.clone()
    };
    let refused_transport = authority(&application_url, &tls);
    assert!(
        EventlogPostgresStore::open(
            registry(),
            config(&schema, prefix, refused_transport),
            admission(),
            refused_authority,
            bridge(),
        )
        .is_err(),
        "an altered physical authority must be refused"
    );

    let reopened_transport = authority(&application_url, &tls);
    let mut reopened = EventlogPostgresStore::open(
        registry(),
        config(&schema, prefix, reopened_transport.clone()),
        admission(),
        exact_authority,
        bridge(),
    )
    .expect("fresh caller-owned authority reopens the exact binding");
    assert_eq!(
        reopened
            .load("ticket", "caller-owned")
            .expect("reopened state")
            .expect("retained instance")
            .revision,
        1
    );
    assert_eq!(reopened.ids("ticket").expect("reopened ids").len(), 3);
    assert_eq!(
        reopened.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    assert!(reopened_transport.calls.load(Ordering::Acquire) >= 1);
    drop_schema(&migration_url, &tls, &schema);
}
