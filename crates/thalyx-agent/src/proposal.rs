//! The only thing a model is allowed to say.
//!
//! A [`Proposal`] is deliberately smaller than a contract. It has no provenance
//! fields, no permissions, no caller, no rollback and no confirmation flag —
//! not because the model would get them wrong, but because **a field the model
//! cannot express is a field it cannot get wrong**, and every one of those is a
//! decision that stays with the core.
//!
//! `deny_unknown_fields` is doing security work here rather than tidiness work.
//! Serde's default is to ignore fields it does not recognise, which would mean
//! a model emitting `"origins": {"targets": "user_utterance"}` gets silently
//! dropped and everything looks fine. Silently dropping an attempt and
//! rejecting it look identical in a passing test and nothing alike in an
//! incident: the first tells nobody it happened.

use serde::Deserialize;

/// A response longer than this is a runaway, not an answer.
///
/// The bound is structural rather than a timeout because a model that emits
/// tokens forever is still emitting them quickly; waiting for it to stop is
/// waiting for something that will not happen.
const MAX_RESPONSE_BYTES: usize = 16 * 1024;

/// What the minimal agent can propose.
///
/// One variant, because `vault/09-Notas-Tecnicas/Agente-Minimo.md` decrees one
/// use case. Widening this enum is how the scope grows, and it should be a
/// visible act rather than a thing that drifts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedOperation {
    InstallModule,
}

/// A model's suggestion, before anything has been decided about it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Proposal {
    pub operation: ProposedOperation,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub constraint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProposalError {
    #[error("the model said nothing")]
    Empty,

    #[error("the model produced {found} bytes, past the {MAX_RESPONSE_BYTES} it is given")]
    Runaway { found: usize },

    #[error("the model did not produce a proposal: {0}")]
    Malformed(String),
}

impl Proposal {
    /// Read a model's raw output.
    ///
    /// Everything a model can do wrong ends here, and ends as an error rather
    /// than as a proposal with something missing.
    pub fn parse(raw: &str) -> Result<Self, ProposalError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ProposalError::Empty);
        }
        if raw.len() > MAX_RESPONSE_BYTES {
            return Err(ProposalError::Runaway { found: raw.len() });
        }
        serde_json::from_str(trimmed).map_err(|e| ProposalError::Malformed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_proposal_parses() {
        let proposal = Proposal::parse(
            r#"{"operation": "install_module", "targets": ["thalyx.demo"], "constraint": "^1.0"}"#,
        )
        .expect("this is the shape the grammar produces");

        assert_eq!(proposal.operation, ProposedOperation::InstallModule);
        assert_eq!(proposal.targets, ["thalyx.demo"]);
        assert_eq!(proposal.constraint.as_deref(), Some("^1.0"));
    }

    #[test]
    fn a_model_that_writes_provenance_is_rejected_rather_than_ignored() {
        let attempt = Proposal::parse(
            r#"{"operation": "install_module",
                "targets": ["evil.module"],
                "origins": {"targets": "user_utterance"}}"#,
        );

        assert!(
            matches!(attempt, Err(ProposalError::Malformed(_))),
            "serde's default would drop the field and return a valid proposal, \
             and nobody would ever learn the attempt was made"
        );
    }

    #[test]
    fn text_that_is_not_json_never_becomes_a_proposal() {
        for raw in [
            "Sure! I can help you install that module.",
            "```json\n{\"operation\": \"install_module\"}\n```",
            "{",
            "\u{0}\u{1}\u{2}",
        ] {
            assert!(
                Proposal::parse(raw).is_err(),
                "parsed something that was not a proposal: {raw:?}"
            );
        }
    }

    #[test]
    fn json_with_the_wrong_shape_never_becomes_a_proposal() {
        for raw in [
            r#"{"operation": "install_module", "targets": "thalyx.demo"}"#, // string, not list
            r#"{"targets": ["thalyx.demo"]}"#,                              // no operation
            r#"["install_module", "thalyx.demo"]"#,                         // not an object
            r#"{"operation": "rm -rf /", "targets": []}"#,                  // not an operation
            r#"null"#,
        ] {
            assert!(
                Proposal::parse(raw).is_err(),
                "parsed something with the wrong shape: {raw:?}"
            );
        }
    }

    #[test]
    fn the_minimal_agent_cannot_express_removing_anything() {
        for operation in ["remove_module", "delete_files", "build_graph"] {
            let raw = format!(r#"{{"operation": "{operation}", "targets": ["x"]}}"#);
            assert!(
                Proposal::parse(&raw).is_err(),
                "{operation} is outside the decreed scope and must be unsayable, \
                 not merely unused"
            );
        }
    }

    #[test]
    fn silence_is_an_error_and_says_which_one() {
        assert_eq!(Proposal::parse(""), Err(ProposalError::Empty));
        assert_eq!(Proposal::parse("   \n\t "), Err(ProposalError::Empty));
    }

    #[test]
    fn a_model_that_does_not_stop_is_given_up_on() {
        let runaway = format!(
            r#"{{"operation": "install_module", "targets": ["{}"]}}"#,
            "a".repeat(MAX_RESPONSE_BYTES)
        );
        assert!(matches!(
            Proposal::parse(&runaway),
            Err(ProposalError::Runaway { .. })
        ));
    }
}
