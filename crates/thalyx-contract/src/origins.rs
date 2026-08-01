//! Per-field provenance.
//!
//! Every field of a contract that has an effect on the system declares where
//! it came from, and the core refuses any of them sourced from content Thalyx
//! does not control.
//!
//! The reason this works where filtering does not: it is a **structural**
//! check. It never reads the text, never judges intent, and never has to keep
//! up with new phrasings. A contract either carries acceptable provenance on
//! its effectful fields or it does not, and that is decidable by a rule that
//! cannot be argued with.
//!
//! See `vault/11-Seguridad/Marcado-de-Origen.md`.

use crate::ContractError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a field's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Directly from what the user said or typed. The most trusted source
    /// there is, because it is the only one that expresses their intent.
    UserUtterance,
    /// From Thalyx's own state: the index, the journal, the permission
    /// registry, the persistent memory.
    SystemState,
    /// From text Thalyx does not control: the community repository,
    /// third-party manifests, network responses, files belonging to others.
    UntrustedContent,
}

impl Origin {
    /// Whether a field carrying this origin may determine what happens.
    pub fn may_have_effect(self) -> bool {
        !matches!(self, Origin::UntrustedContent)
    }
}

impl From<Origin> for thalyx_journal::Origin {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::UserUtterance => thalyx_journal::Origin::UserUtterance,
            Origin::SystemState => thalyx_journal::Origin::SystemState,
            Origin::UntrustedContent => thalyx_journal::Origin::UntrustedContent,
        }
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Origin::UserUtterance => "user_utterance",
            Origin::SystemState => "system_state",
            Origin::UntrustedContent => "untrusted_content",
        };
        f.write_str(s)
    }
}

/// The fields whose value decides what the system does.
///
/// Kept as an explicit list rather than derived from the struct: which fields
/// have effect is a security decision, so adding one must be a deliberate act
/// that shows up in a diff, not a side effect of adding a field.
pub const EFFECTFUL_FIELDS: [&str; 4] = ["operation", "targets", "permissions", "constraint"];

/// The provenance map carried by every contract.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Origins(BTreeMap<String, Origin>);

impl Origins {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn set(&mut self, field: &str, origin: Origin) -> &mut Self {
        self.0.insert(field.to_string(), origin);
        self
    }

    pub fn get(&self, field: &str) -> Option<Origin> {
        self.0.get(field).copied()
    }

    /// Every effectful field must be present and must be allowed to have effect.
    ///
    /// Absence is a rejection, not a default. A field whose origin was never
    /// recorded and one whose origin was stripped look identical from here, so
    /// treating absence as trusted would make the mechanism opt-in for whoever
    /// wants to evade it.
    pub fn validate(&self) -> Result<(), ContractError> {
        for field in EFFECTFUL_FIELDS {
            match self.0.get(field) {
                None => return Err(ContractError::MissingOrigin { field }),
                Some(origin) if !origin.may_have_effect() => {
                    return Err(ContractError::UntrustedOrigin { field });
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// The least trusted origin present, or `UserUtterance` if empty.
    ///
    /// A contract is only as trustworthy as its weakest field, and this is
    /// what gets recorded in the journal so an audit reads the floor rather
    /// than the ceiling.
    pub fn least_trusted(&self) -> Origin {
        self.0
            .values()
            .copied()
            .max()
            .unwrap_or(Origin::UserUtterance)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Origin)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete() -> Origins {
        let mut origins = Origins::new();
        for field in EFFECTFUL_FIELDS {
            origins.set(field, Origin::UserUtterance);
        }
        origins
    }

    #[test]
    fn a_complete_trusted_map_validates() {
        assert!(complete().validate().is_ok());
    }

    #[test]
    fn every_effectful_field_is_required() {
        for missing in EFFECTFUL_FIELDS {
            let mut origins = Origins::new();
            for field in EFFECTFUL_FIELDS {
                if field != missing {
                    origins.set(field, Origin::UserUtterance);
                }
            }
            assert_eq!(
                origins.validate(),
                Err(ContractError::MissingOrigin { field: missing })
            );
        }
    }

    #[test]
    fn untrusted_content_never_has_effect() {
        assert!(!Origin::UntrustedContent.may_have_effect());
        assert!(Origin::UserUtterance.may_have_effect());
        assert!(Origin::SystemState.may_have_effect());
    }

    #[test]
    fn extra_fields_may_carry_any_origin() {
        let mut origins = complete();
        origins.set("description", Origin::UntrustedContent);
        origins.set("summary", Origin::UntrustedContent);
        assert!(
            origins.validate().is_ok(),
            "untrusted text is allowed to inform what is shown, just not what is done"
        );
    }

    #[test]
    fn least_trusted_reports_the_floor_not_the_ceiling() {
        let mut origins = complete();
        assert_eq!(origins.least_trusted(), Origin::UserUtterance);

        origins.set("constraint", Origin::SystemState);
        assert_eq!(origins.least_trusted(), Origin::SystemState);

        origins.set("description", Origin::UntrustedContent);
        assert_eq!(
            origins.least_trusted(),
            Origin::UntrustedContent,
            "one untrusted field is enough to lower the whole contract"
        );
    }

    #[test]
    fn origins_survive_a_json_round_trip() {
        let origins = complete();
        let json = serde_json::to_string(&origins).unwrap();
        let back: Origins = serde_json::from_str(&json).unwrap();
        assert_eq!(origins, back);
        assert!(json.contains("user_utterance"));
    }

    #[test]
    fn the_effectful_list_is_deliberate() {
        // If this test fails because a field was added, that is the point:
        // extending what counts as effectful has to be a decision someone
        // makes, not something that happens by accident.
        assert_eq!(
            EFFECTFUL_FIELDS,
            ["operation", "targets", "permissions", "constraint"]
        );
    }
}
