//! Thin dispatch only; expected observations belong exclusively to admitted suites.
use crate::{common, core, executor, query, shell, store};
use ess_conformance::target::*;
use std::cell::RefCell;

struct Context {
    scenario: String,
    core: core::Target,
    query: query::Target,
    shell: shell::Target,
    store: store::Target,
    executor: executor::Target,
    #[cfg(feature = "eventlog")]
    provider: crate::provider::Target,
}

pub struct Target {
    implementation: String,
    context: RefCell<Option<Context>>,
    observations: RefCell<Vec<serde_json::Value>>,
}

impl Target {
    pub fn new(implementation: String) -> Self {
        Self {
            implementation,
            context: RefCell::new(None),
            observations: RefCell::new(Vec::new()),
        }
    }
    pub fn observations(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&*self.observations.borrow())
    }
}

impl ConformanceTarget for Target {
    fn identity(&self) -> Result<ImplementationIdentity, TargetError> {
        Ok(ImplementationIdentity::new(
            "entity-runtime",
            &self.implementation,
        ))
    }
    fn begin_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        *self.context.borrow_mut() = Some(Context {
            scenario: scenario.scenario.to_string(),
            core: core::Target::new()?,
            query: query::Target::new()?,
            shell: shell::Target::new()?,
            store: store::Target::new()?,
            executor: executor::Target::new()?,
            #[cfg(feature = "eventlog")]
            provider: crate::provider::Target::new()?,
        });
        Ok(())
    }
    fn end_scenario(&self, _: &ScenarioContext) -> Result<(), TargetError> {
        *self.context.borrow_mut() = None;
        Ok(())
    }
    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        let name = request.command.to_string();
        let mut context = self.context.borrow_mut();
        let context = context
            .as_mut()
            .ok_or_else(|| common::unavailable("scenario not open"))?;
        let (domain, operation) = name
            .rsplit_once('.')
            .ok_or_else(|| common::unavailable("unqualified command"))?;
        let fields = match domain {
            "entity.core" => context.core.execute(operation, &request.input)?,
            "entity.shell" => context.shell.execute(operation, &request.input)?,
            "entity.query" => context.query.execute(operation, &request.input)?,
            "entity.store" => context.store.execute(operation, &request.input)?,
            "entity.executor" => context.executor.execute(operation, &request.input)?,
            #[cfg(feature = "eventlog")]
            "entity-provider.tracking" => context.provider.execute(operation, &request.input)?,
            _ => return Err(TargetError::unsupported(&name, "unknown library domain")),
        };
        self.observations.borrow_mut().push(
            serde_json::json!({"scenario":context.scenario,"command":name,"response":fields}),
        );
        let mut result = SemanticCommandResult::took(ess_conformance::scenario::OutcomeRef::new(
            request.command,
            "returned".parse().map_err(common::unavailable)?,
        ));
        result.response = Some(fields);
        Ok(result)
    }
    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        Err(TargetError::unsupported(
            request.view.to_string(),
            "library reads are real function returns",
        ))
    }
    fn observe_events(
        &self,
        request: EventObservationRequest,
    ) -> Result<Vec<ObservedEvent>, TargetError> {
        Err(TargetError::unsupported(
            request.event.to_string(),
            "libraries return events; they do not publish them",
        ))
    }
    fn configure_external_outcome(
        &self,
        request: ExternalOutcomeControl,
    ) -> Result<(), TargetError> {
        Err(TargetError::unsupported(
            request.force.to_string(),
            "no manufactured outcomes",
        ))
    }
    fn redeliver_event(&self, request: RedeliveryRequest) -> Result<(), TargetError> {
        Err(TargetError::unsupported(
            request.event.to_string(),
            "no event transport",
        ))
    }
}
