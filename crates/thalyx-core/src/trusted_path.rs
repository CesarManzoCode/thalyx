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
//! ## The manifest is signed, which is not the same as trustworthy
//!
//! "There is no parameter through which arbitrary text could reach it" was
//! true and was not enough. The prompt interpolates the module's `name`, its
//! id, its version and its permission resources, and every one of those is a
//! string a publisher wrote. A signature says who wrote it; it says nothing
//! about what they wrote.
//!
//! So a publisher — self-signed, which is all trust-on-first-use requires for
//! a new id — could put a newline and a box-drawing character in the module's
//! name and paint extra lines inside the frame. Or an ANSI escape and repaint
//! the whole prompt. The frame exists precisely so the human can tell Thalyx
//! apart from everything running inside it, and the frame was drawing whatever
//! it was handed.
//!
//! Every untrusted field now goes through [`sanitise`] on its way into the
//! prompt: one line, no control characters, no escapes, bounded length. The
//! banner is the security property, so nothing a publisher writes may reach
//! the part of the screen that draws it.

use thalyx_manifest::{Manifest, Permission};

/// The longest a publisher-supplied string may be inside a prompt.
///
/// Long enough for any honest name, short enough that no single field can
/// push the permission list off a terminal — which is the same attack as
/// hiding a permission, performed with length instead of escapes.
///
/// **This applies to labels, never to what is being authorised.** See
/// [`sanitise_permission`], and the note there about why the difference is not
/// a matter of taste.
const MAX_FIELD: usize = 72;

/// How wide a wrapped permission line is drawn.
const WRAP_AT: usize = 60;

/// How many lines one permission may occupy before its middle is elided.
///
/// Six is past any real path and short of a screen. A publisher who writes a
/// four-thousand-character resource still cannot scroll the prompt away.
const MAX_WRAPPED_LINES: usize = 6;

/// Make a publisher-supplied string safe to draw inside the frame.
///
/// Three things, and each one is a way the frame could otherwise be forged:
///
/// - **Control characters become `·`.** A newline ends the line and lets the
///   next one start wherever the publisher likes, including with a `│` that
///   makes forged content look like Thalyx's own. A carriage return rewrites
///   the line already drawn. `\x1b` starts an escape sequence that can move
///   the cursor anywhere on the screen and recolour anything.
/// - **Truncated to [`MAX_FIELD`].** With an ellipsis, so a name that was cut
///   does not read as the whole name.
/// - **Empty becomes a placeholder.** A blank field would leave a line that
///   looks like a formatting accident rather than a value nobody supplied.
///
/// Replacing rather than stripping is deliberate: a stripped character leaves
/// no evidence, and `a·b` reading oddly is how somebody notices that a name
/// contained something it should not have.
pub fn sanitise(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { '·' } else { c })
        .collect();

    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return "(blank)".to_string();
    }

    if cleaned.chars().count() > MAX_FIELD {
        let kept: String = cleaned.chars().take(MAX_FIELD - 1).collect();
        format!("{kept}…")
    } else {
        cleaned.to_string()
    }
}

/// Render one permission across as many lines as it honestly needs.
///
/// ## Why this is not just [`sanitise`]
///
/// It was, and that was a bug introduced by the fix for the forgery one. A
/// permission is **what the human is authorising**, and truncating it hides
/// exactly the part that distinguishes one grant from another: cut
/// `/home/user/projects/secrets` at seventy-two characters on a machine with a
/// long home directory and it becomes `/home/user/…`, which is also what
/// `/home/user/projects/public` becomes. The human then confirms a sentence
/// that is true of both.
///
/// Under-reporting is the dangerous direction — the module docs say so about
/// showing a subset of the permissions, and a truncated permission is the same
/// failure inside one line. So the name, the id and the version may be cut,
/// because they are labels; this may not, because it is the decision.
///
/// Length is still bounded, or a publisher could push the rest of the list off
/// the screen with one enormous resource. Past [`MAX_WRAPPED_LINES`] the
/// **middle** is elided rather than the end, so both the root of the path and
/// its final component survive — the two parts that say what is being granted.
pub fn sanitise_permission(permission: &Permission) -> Vec<String> {
    let text = sanitise_control(&permission.describe());
    if text.is_empty() {
        return vec!["(blank)".to_string()];
    }

    let characters: Vec<char> = text.chars().collect();
    let budget = WRAP_AT * MAX_WRAPPED_LINES;

    let shown: String = if characters.len() <= budget {
        text
    } else {
        // Keep the head and the tail. The tail is not decoration: for a path it
        // is the leaf, which is the whole of what distinguishes two grants that
        // share a parent.
        let tail = WRAP_AT.min(characters.len() / 3);
        let head = budget.saturating_sub(tail + 3);
        format!(
            "{}…{}",
            characters[..head].iter().collect::<String>(),
            characters[characters.len() - tail..]
                .iter()
                .collect::<String>()
        )
    };

    shown
        .chars()
        .collect::<Vec<char>>()
        .chunks(WRAP_AT)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Replace control characters, without touching length.
///
/// The half of [`sanitise`] that is about forgery rather than about fitting on
/// a screen. Split out because the permission renderer needs the first and must
/// not have the second.
fn sanitise_control(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '·' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

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
        out.push_str(&format!(
            "│ {} ({})\n",
            sanitise(&self.module_name),
            sanitise(&self.module_id)
        ));
        out.push_str(&format!("│ version {}\n", sanitise(&self.version)));
        out.push_str("│\n");
        out.push_str("│ This module permanently requests:\n");
        for permission in &self.permissions {
            // `describe` interpolates the resource, which is a publisher
            // string. The sentence around it is Thalyx's; what it names is not.
            //
            // Wrapped rather than truncated, because this line *is* the
            // decision. See `sanitise_permission`.
            for (index, line) in sanitise_permission(permission).iter().enumerate() {
                let marker = if index == 0 { "·" } else { " " };
                out.push_str(&format!("│   {marker} {line}\n"));
            }
        }
        out.push_str("│\n");
        out.push_str("│ These permissions come from the module's signed manifest.\n");
        out.push_str("│ They stay in force until you revoke them by hand.\n");
        out.push_str("└──────────────────────────────────────────────────────");
        out
    }
}

/// Sanitise text that is allowed to be several lines, as a list of lines.
///
/// For what a module says over its channel: a notice may legitimately want
/// more than one line, and the caller has to prefix each of them with the
/// marker that says who is speaking. Returning lines rather than a string is
/// what forces that — a single string could be printed with one prefix and the
/// rest of the lines would carry none, which is the forgery the marker exists
/// to prevent.
///
/// Bounded, because "several" is not "unlimited": a module cannot scroll the
/// human's screen with one notice.
pub fn sanitise_block(text: &str) -> Vec<String> {
    const MAX_LINES: usize = 8;

    let mut lines: Vec<String> = text.lines().take(MAX_LINES).map(sanitise).collect();

    if text.lines().count() > MAX_LINES {
        lines.push(format!(
            "… and {} more line(s)",
            text.lines().count() - MAX_LINES
        ));
    }
    if lines.is_empty() {
        lines.push(sanitise(""));
    }
    lines
}

/// How an agent's own prose must be shown: separated, and labelled untrusted.
///
/// Never merged into a [`CapabilityPrompt`].
pub fn render_untrusted_note(text: &str) -> String {
    let mut out = String::from("· agent (not verified by Thalyx):\n");
    for line in text.lines() {
        // Sanitised for the same reason the prompt's fields are. Labelling
        // text as untrusted and then letting it emit escape sequences would
        // label it in a way it could paint over.
        out.push_str(&format!("    {}\n", sanitise(line)));
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
    use thalyx_manifest::PermissionKind;

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
    fn a_publisher_cannot_draw_extra_lines_inside_the_frame() {
        // The frame is the security property: it is how the human tells Thalyx
        // apart from anything running inside it. A module name carrying a
        // newline and a `│` would paint lines that look like Thalyx's own —
        // and a manifest is signed by whoever wrote it, which for a new id is
        // anybody at all.
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "/home/user/docs"
action   = "read"
type     = "persistent"
"#,
        );
        let honest = CapabilityPrompt::for_manifest(&manifest).unwrap();
        let honest_lines = honest.render().lines().count();

        let mut forged = CapabilityPrompt::for_manifest(&manifest).unwrap();
        forged.module_name = "Innocent\n│ This module requests: nothing at all\n│ Safe".to_string();
        let rendered = forged.render();

        // The claim is not that the publisher's words vanish — they are the
        // module's name and the human should see them. It is that the publisher
        // cannot add a *line*. The frame's shape is Thalyx's alone, so the same
        // prompt with a hostile name has exactly the same number of lines.
        assert_eq!(
            rendered.lines().count(),
            honest_lines,
            "a publisher's newline started a line of its own inside the frame:\n{rendered}"
        );
        assert!(
            rendered.contains("Innocent·"),
            "the control characters should be visible, not silently removed: {rendered}"
        );
    }

    #[test]
    fn a_publisher_cannot_repaint_the_screen_with_an_escape_sequence() {
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "/home/user/docs"
action   = "read"
type     = "persistent"
"#,
        );
        let mut prompt = CapabilityPrompt::for_manifest(&manifest).unwrap();
        prompt.module_name = "\u{1b}[2J\u{1b}[H Thalyx — everything is fine".to_string();

        let rendered = prompt.render();
        assert!(
            !rendered.contains('\u{1b}'),
            "an escape sequence reached the terminal through the trusted path"
        );
    }

    #[test]
    fn an_enormous_field_cannot_push_the_permissions_off_the_screen() {
        // Hiding a permission by length rather than by escapes. The list has
        // to still be there and still be readable.
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"
"#,
        );
        let mut prompt = CapabilityPrompt::for_manifest(&manifest).unwrap();
        prompt.module_name = "A".repeat(10_000);

        let rendered = prompt.render();
        assert!(rendered.contains("outbound network access"));
        for line in rendered.lines() {
            assert!(
                line.chars().count() < 120,
                "a line grew to {} characters",
                line.chars().count()
            );
        }
    }

    #[test]
    fn a_permission_resource_is_sanitised_as_well_as_the_name() {
        // The resource is a publisher string too, and it reaches the screen
        // through `describe`. Sanitising the name alone would move the hole
        // rather than close it.
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "/home/user/docs"
action   = "read"
type     = "persistent"
"#,
        );
        let mut prompt = CapabilityPrompt::for_manifest(&manifest).unwrap();

        // Set here rather than through TOML, because TOML refuses a raw escape
        // in a basic string. That is a small mercy and not a defence: `\u001b`
        // is a legal TOML escape and produces exactly the same byte.
        prompt.permissions[0].resource = "/home/user/\u{1b}[2Kdocs".to_string();

        let rendered = prompt.render();
        assert!(
            !rendered.contains('\u{1b}'),
            "an escape reached the screen through a permission resource"
        );
        assert!(rendered.contains("\u{b7}[2Kdocs"));
    }

    #[test]
    fn two_grants_that_share_a_long_prefix_are_still_told_apart() {
        // The bug the first version of the sanitiser introduced, and the reason
        // a permission is wrapped rather than cut.
        //
        // A machine with a long home directory pushes the distinguishing part
        // of a path past any fixed width. Truncating there makes
        // `.../projects/public` and `.../projects/secrets` render identically,
        // and the human confirms a sentence that is true of both. Hiding what
        // is being authorised is the same failure as showing a subset of the
        // permissions, performed inside one line.
        let prefix = "/home/a-rather-long-user-name/nested/deeply/inside/projects";

        let public = Permission {
            resource: format!("{prefix}/public"),
            action: "read".to_string(),
            kind: PermissionKind::Persistent,
        };
        let secrets = Permission {
            resource: format!("{prefix}/secrets"),
            action: "read".to_string(),
            kind: PermissionKind::Persistent,
        };

        let shown = |permission| sanitise_permission(permission).join("");

        assert_ne!(
            shown(&public),
            shown(&secrets),
            "two different grants render identically, so confirming one confirms either"
        );
        assert!(shown(&public).contains("public"));
        assert!(shown(&secrets).contains("secrets"));
    }

    #[test]
    fn an_enormous_resource_keeps_both_its_root_and_its_leaf() {
        // Bounded, still. A publisher must not be able to push the rest of the
        // list off the screen with one resource — but what survives the bound
        // has to be the two parts that say what is being granted, so the middle
        // goes and the ends stay.
        let permission = Permission {
            resource: format!("/home/user/{}/target-directory", "x".repeat(4000)),
            action: "write".to_string(),
            kind: PermissionKind::Persistent,
        };

        let lines = sanitise_permission(&permission);
        assert!(
            lines.len() <= MAX_WRAPPED_LINES,
            "one permission took {} lines",
            lines.len()
        );

        let shown = lines.join("");
        assert!(shown.contains("/home/user/"), "the root was lost: {shown}");
        assert!(
            shown.contains("target-directory"),
            "the leaf was lost, which is what distinguishes one grant from another"
        );
    }

    #[test]
    fn a_wrapped_permission_cannot_forge_a_second_bullet() {
        // Wrapping adds lines, and a line the publisher influences must not be
        // able to look like a new permission. Only the first line carries the
        // bullet; the continuations are indented under it.
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"
"#,
        );
        let mut prompt = CapabilityPrompt::for_manifest(&manifest).unwrap();
        prompt.permissions[0].resource = "/home/user/".to_string() + &"a".repeat(200);

        let rendered = prompt.render();
        let bullets = rendered.lines().filter(|line| line.contains("· ")).count();
        assert_eq!(
            bullets, 1,
            "a single permission drew {bullets} bullets:\n{rendered}"
        );
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
