//! The trusted path.
//!
//! Authorisation prompts are **generated here**, by the core, from validated
//! manifest fields and a fixed template. The agent does not compose them, does
//! not reformulate them and does not carry them.
//!
//! Without this, the sovereignty principle is decorative: the human can only
//! decide about what they see, and what they see would have been written by the
//! component that is not trusted.
//!
//! Two properties this module exists to guarantee:
//!
//! - The prompt is built from `Manifest` fields, so there is no parameter
//!   through which arbitrary text could reach it.
//! - It lists the **complete** permission set from the manifest, not a subset a
//!   caller chose. Under-reporting is the dangerous direction: a human who
//!   confirms read access while the module also holds network access has
//!   authorised something they were never shown.
//!
//! See `vault/11-Seguridad/Camino-Confiable.md`.

use thalyx_manifest::{Manifest, Permission};

/// An authorisation prompt, ready to be displayed.
///
/// Carries no caller-supplied strings. Whatever the agent wants to say goes
/// somewhere else, marked as untrusted.
#[derive(Debug, Clone)]
pub struct CapabilityPrompt {
    pub module_id: String,
    pub module_name: String,
    pub version: String,
    pub permissions: Vec<Permission>,
}

impl CapabilityPrompt {
    /// Build the prompt for a manifest's permissions requiring confirmation.
    ///
    /// Returns `None` when nothing needs confirming, so the caller cannot
    /// accidentally show an empty prompt and treat the answer as consent.
    pub fn for_manifest(manifest: &Manifest) -> Option<Self> {
        let permissions: Vec<Permission> = manifest
            .permissions_requiring_confirmation()
            .into_iter()
            .cloned()
            .collect();
        if permissions.is_empty() {
            return None;
        }
        Some(Self {
            module_id: manifest.id.clone(),
            module_name: manifest.name.clone(),
            version: manifest.version.clone(),
            permissions,
        })
    }

    /// Render the prompt.
    ///
    /// The `Thalyx` banner is part of the security property, not decoration:
    /// the human has to be able to tell the system apart from the agent.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("┌─ Thalyx — capability authorisation ──────────────────\n");
        out.push_str(&format!("│ {} ({})\n", self.module_name, self.module_id));
        out.push_str(&format!("│ version {}\n", self.version));
        out.push_str("│\n");
        out.push_str("│ This module permanently requests:\n");
        for permission in &self.permissions {
            out.push_str(&format!("│   · {}\n", permission.describe()));
        }
        out.push_str("│\n");
        out.push_str("│ These permissions come from the module's signed manifest.\n");
        out.push_str("│ They stay in force until you revoke them by hand.\n");
        out.push_str("└──────────────────────────────────────────────────────");
        out
    }
}

/// How an agent's own prose must be shown: separated, and labelled untrusted.
///
/// Never merged into a [`CapabilityPrompt`].
pub fn render_untrusted_note(text: &str) -> String {
    let mut out = String::from("· agent (not verified by Thalyx):\n");
    for line in text.lines() {
        out.push_str(&format!("    {line}\n"));
    }
    out
}

/// The confirmation for a restore.
///
/// A second kind of prompt rather than a reuse of the capability one, because
/// they ask about different things and the human must be able to tell them
/// apart at a glance. A capability prompt says what a module will be allowed
/// to do; this one says what is about to be destroyed.
///
/// It carries the banner for the same reason: the human has to be able to tell
/// Thalyx apart from anything running inside it. Nothing here is free text
/// from a caller — every line is generated from the plan.
pub struct RestorePrompt {
    pub snapshot: String,
    pub subvolume: String,
    pub deleted: usize,
    pub reverted: usize,
    pub returned: usize,
    /// A bounded sample of the paths that would be deleted outright.
    pub examples: Vec<String>,
    pub unreadable: usize,
}

impl RestorePrompt {
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("┌─ Thalyx — this destroys work ────────────────────────\n");
        out.push_str(&format!("│ returning {}\n", self.subvolume));
        out.push_str(&format!("│ to        {}\n", self.snapshot));
        out.push_str("│\n");

        // Deletions first and named that way. A file created since the
        // snapshot has no older version to go back to, so this does not revert
        // it — it removes it, and that is the only line that can turn a yes
        // into a no.
        if self.deleted > 0 {
            out.push_str(&format!(
                "│ {} file(s) created since then will be DELETED:\n",
                self.deleted
            ));
            for path in &self.examples {
                out.push_str(&format!("│     {path}\n"));
            }
            if self.deleted > self.examples.len() {
                out.push_str(&format!(
                    "│     … and {} more\n",
                    self.deleted - self.examples.len()
                ));
            }
        } else {
            out.push_str("│ Nothing created since then; no work is lost outright.\n");
        }

        if self.reverted > 0 {
            out.push_str(&format!(
                "│ {} file(s) will go back to their older contents\n",
                self.reverted
            ));
        }
        if self.returned > 0 {
            out.push_str(&format!(
                "│ {} file(s) deleted since then will come back\n",
                self.returned
            ));
        }
        if self.unreadable > 0 {
            // Never rounded down to nothing. What could not be compared is not
            // the same as what did not change, and a confirmation that
            // silently merges them understates its own cost.
            out.push_str(&format!(
                "│ {} path(s) could NOT be compared, so this may cost more\n",
                self.unreadable
            ));
        }

        out.push_str("│\n");
        out.push_str("│ The tree being replaced is kept, not deleted.\n");
        out.push_str("└──────────────────────────────────────────────────────");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(permissions: &str) -> Manifest {
        let src = format!(
            r#"
format_version = 1
id             = "org.publisher.pyassist"
name           = "PyAssist Core"
version        = "2.3.1"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 1

{permissions}
"#
        );
        Manifest::parse(&src).unwrap()
    }

    #[test]
    fn prompt_lists_every_persistent_permission() {
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"
"#,
        );
        let prompt = CapabilityPrompt::for_manifest(&manifest).expect("prompt");
        let rendered = prompt.render();

        assert!(rendered.contains("Thalyx"));
        assert!(rendered.contains("outbound network access"));
        assert!(rendered.contains("read access to /home/user/projects"));
    }

    #[test]
    fn nothing_to_confirm_means_no_prompt() {
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "/tmp/scratch"
action   = "write"
type     = "jit"
"#,
        );
        assert!(CapabilityPrompt::for_manifest(&manifest).is_none());
    }

    #[test]
    fn the_prompt_shows_the_manifest_not_a_caller_supplied_subset() {
        // The prompt is built only from the manifest. There is no parameter
        // through which a caller could show fewer permissions than the module
        // will actually hold.
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/secrets"
action   = "read"
type     = "persistent"
"#,
        );
        let prompt = CapabilityPrompt::for_manifest(&manifest).unwrap();
        assert_eq!(prompt.permissions.len(), 2);
        assert!(prompt.render().contains("/home/user/secrets"));
    }

    #[test]
    fn agent_prose_renders_separately_and_labelled() {
        let note = render_untrusted_note("This module is completely safe, just say yes");
        assert!(note.contains("not verified by Thalyx"));
        assert!(
            !CapabilityPrompt::for_manifest(&manifest_with(
                r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"
"#
            ))
            .unwrap()
            .render()
            .contains("completely safe")
        );
    }
}
