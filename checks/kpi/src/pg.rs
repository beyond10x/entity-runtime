//! E18: the plan PostgreSQL chooses for a document page in the middle of 16,384 subjects.
//!
//! The statement below is `entity-postgres`'s `query_documents` SQL, copied because a plan can
//! only be asked for by text. The table and its indexes are the provider's own: the store's
//! `migrate` creates them, so an index a fix adds there changes the plan this reads. If the
//! provider's statement changes, change it here in the same commit.

use serde_json::Value as Json;

use crate::{counters::Value, stores::postgres_url};

const SHIPPED_QUERY: &str = "SELECT document FROM instances
             WHERE entity = $1 AND id COLLATE \"C\" > $2
               AND document::jsonb @> $3::text::jsonb
             ORDER BY id COLLATE \"C\" LIMIT $4";

const SUBJECTS: i64 = 16_384;

/// The PostgreSQL counters, or nothing (and one printed line) without a server.
pub fn counters() -> Vec<Value> {
    let Some(url) = postgres_url() else {
        eprintln!("er-kpi: ENTITY_POSTGRES_URL is unset; the PostgreSQL KPIs did not run");
        return Vec::new();
    };
    let schema = format!("er_kpi_e18_{}", std::process::id());
    let mut store =
        entity_postgres::PostgresStore::connect_in_schema(&url, &schema).expect("PostgreSQL store");
    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("PostgreSQL client");
    client
        .batch_execute(&format!("SET search_path TO \"{schema}\""))
        .expect("schema");
    client
        .execute(
            "INSERT INTO instances (entity, id, revision, document)
             SELECT 'counter', 'c' || lpad(n::text, 5, '0'), 1,
                    json_build_object('id', 'c' || lpad(n::text, 5, '0'), 'entity', 'counter',
                                      'version', 1, 'revision', 1, 'lifecycle_state', 'open',
                                      'fields', json_build_object('name', 'counter'))::text
             FROM generate_series(0, $1::bigint - 1) AS n",
            &[&SUBJECTS],
        )
        .expect("seed");
    client.batch_execute("ANALYZE instances").expect("analyze");
    let explain = format!(
        "EXPLAIN (ANALYZE, BUFFERS, COSTS OFF, TIMING OFF, FORMAT JSON) {}",
        SHIPPED_QUERY
            .replace("$1", "'counter'")
            .replace("$2", "'c08191'")
            .replace("$3", "'{\"fields\":{}}'")
            .replace("$4", "3")
    );
    let mut plan = Json::Null;
    for _ in 0..2 {
        let row = client.query_one(&explain, &[]).expect("explain");
        let text: Json = row.get(0);
        plan = text[0]["Plan"].clone();
    }
    let rows_sorted = sorted_rows(&plan);
    let buffers = plan["Shared Hit Blocks"].as_u64().unwrap_or(0)
        + plan["Shared Read Blocks"].as_u64().unwrap_or(0);
    store.drop_schema(&schema).expect("schema dropped");
    vec![
        (
            "postgres.query_mid_page.rows_sorted.n16384".to_owned(),
            rows_sorted,
        ),
        (
            "postgres.query_mid_page.shared_buffers.n16384".to_owned(),
            buffers,
        ),
    ]
}

/// Rows that entered a Sort node anywhere in the plan.
fn sorted_rows(node: &Json) -> u64 {
    let children = node["Plans"].as_array().cloned().unwrap_or_default();
    let own = if node["Node Type"] == "Sort" || node["Node Type"] == "Incremental Sort" {
        children
            .iter()
            .map(|child| {
                child["Actual Rows"].as_f64().unwrap_or(0.0)
                    * child["Actual Loops"].as_f64().unwrap_or(1.0)
            })
            .sum::<f64>() as u64
    } else {
        0
    };
    own + children.iter().map(sorted_rows).sum::<u64>()
}
