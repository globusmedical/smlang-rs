use crate::parser::transition::visit_guards;
use crate::parser::{AsyncIdent, ParsedStateMachine};
use proc_macro2::Span;
use std::collections::HashMap;
use syn::parse;

/// A basic representation an action call signature.
#[derive(PartialEq, Clone)]
struct FunctionSignature {
    // Function input arguments.
    arguments: Vec<syn::Type>,

    // Function result (if any).
    result: Option<syn::Type>,

    // Is the function async
    is_async: bool,
}

impl FunctionSignature {
    pub fn new(
        input_data: Option<&syn::Type>,
        event_data: Option<&syn::Type>,
        output_data: Option<&syn::Type>,
        is_async: bool,
    ) -> Self {
        let mut input_arguments = vec![];

        if let Some(datatype) = input_data {
            input_arguments.push(datatype.clone());
        }

        if let Some(datatype) = event_data {
            input_arguments.push(datatype.clone());
        }

        let result = output_data.cloned();

        Self {
            arguments: input_arguments,
            result,
            is_async,
        }
    }

    pub fn new_guard(
        input_state: Option<&syn::Type>,
        event: Option<&syn::Type>,
        is_async: bool,
    ) -> Self {
        // Guards never have output data.
        Self::new(input_state, event, None, is_async)
    }
}

// Verify action and guard function signatures.
fn validate_action_signatures(sm: &ParsedStateMachine) -> Result<(), parse::Error> {
    // Collect all of the action call signatures.
    let mut actions = HashMap::new();

    let all_transitions = &sm.states_events_mapping;

    for (in_state_name, from_transitions) in all_transitions.iter() {
        let in_state_data = sm.state_data.data_types.get(in_state_name);

        for (out_state_name, event_mapping) in from_transitions.iter() {
            let out_state_data = sm.state_data.data_types.get(out_state_name);

            // Get the data associated with this event.
            let event_data = sm
                .event_data
                .data_types
                .get(&event_mapping.event.to_string());
            for transition in &event_mapping.transitions {
                if let Some(AsyncIdent {
                    ident: action,
                    is_async,
                }) = &transition.action
                {
                    let signature = FunctionSignature::new(
                        in_state_data,
                        event_data,
                        out_state_data,
                        *is_async,
                    );

                    // If the action is not yet known, add it to our tracking list.
                    actions
                        .entry(action.to_string())
                        .or_insert_with(|| signature.clone());

                    // Check that the call signature is equivalent to the recorded signature for this
                    // action.
                    if actions.get(&action.to_string()).unwrap() != &signature {
                        return Err(parse::Error::new(
                            Span::call_site(),
                            format!("Action `{}` can only be reused when all input states, events, and output states have the same data", action),
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_guard_signatures(sm: &ParsedStateMachine) -> Result<(), parse::Error> {
    // Collect all of the guard call signatures.
    let mut guards = HashMap::new();

    let all_transitions = &sm.states_events_mapping;

    for (in_state_name, from_transitions) in all_transitions.iter() {
        let in_state_data = sm.state_data.data_types.get(in_state_name);

        for (_out_state_name, event_mapping) in from_transitions.iter() {
            // Get the data associated with this event.
            let event_data = sm
                .event_data
                .data_types
                .get(&event_mapping.event.to_string());
            for transition in &event_mapping.transitions {
                if let Some(guard_expression) = &transition.guard {
                    let res = visit_guards(guard_expression, |guard| {
                        let signature =
                            FunctionSignature::new_guard(in_state_data, event_data, guard.is_async);

                        // If the action is not yet known, add it to our tracking list.
                        guards
                            .entry(guard.ident.to_string())
                            .or_insert_with(|| signature.clone());

                        // Check that the call signature is equivalent to the recorded signature for this
                        // guard.
                        if guards.get(&guard.ident.to_string()).unwrap() != &signature {
                            return Err(parse::Error::new(
                                Span::call_site(),
                                format!("Guard `{}` can only be reused when all input states and events have the same data", guard.ident),
                            ));
                        }
                        Ok(())
                    });
                    res?;
                }
            }
        }
    }

    Ok(())
}
fn validate_unreachable_transitions(sm: &ParsedStateMachine) -> Result<(), parse::Error> {
    let all_transitions = &sm.states_events_mapping;
    for (in_state, event_mappings) in all_transitions {
        for (event, event_mapping) in event_mappings {
            // more than single transition for (in_state,event)
            if event_mapping.transitions.len() > 1 {
                let mut unguarded_count = 0;
                for t in &event_mapping.transitions {
                    if let Some(g) = &t.guard {
                        if unguarded_count > 0 {
                            // Guarded transition AFTER an unguarded one
                            return Err(parse::Error::new(
                                Span::call_site(),
                                format!("{} + {}: [{}] : guarded transition is unreachable because it follows an unguarded transition, which handles all cases",
                                        in_state, event, g),
                            ));
                        }
                    } else {
                        // unguarded
                        unguarded_count += 1;
                        if unguarded_count > 1 {
                            return Err(parse::Error::new(
                                Span::call_site(),
                                format!("{} + {}: State and event combination specified multiple times, remove duplicates.", in_state, event),
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validate coherency of the state machine.
pub fn validate(sm: &ParsedStateMachine) -> Result<(), parse::Error> {
    validate_action_signatures(sm)?;
    validate_guard_signatures(sm)?;
    validate_unreachable_transitions(sm)?;
    Ok(())
}
