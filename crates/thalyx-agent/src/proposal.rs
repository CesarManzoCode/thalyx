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

/// What a model is allowed to propose.
///
/// One variant per verb the session has, plus [`ProposedOperation::Nothing`].
/// Cesar's decree of 2026-08-23, answering the question
/// `vault/02-Arquitectura/Superficie-para-el-LLM.md` had left open: the model
/// may propose the whole catalogue, and the terminal confirmation stays the
/// barrier rather than the grammar being it.
///
/// Widening this enum is still how the scope grows, and it is still meant to be
/// a visible act. What changed is where the line is drawn — not at what the
/// model may **say**, but at what the machine does without a human at a
/// terminal saying yes.
///
/// ## Why the names are the CLI's `op` and not a vocabulary of their own
///
/// The catalogue in `thalyx-cli` declares an `op` for every verb, and that is
/// the string a program already matches on. A second set of names here would
/// be a second thing to keep in step, and `thalyx-cli` has a test that binds
/// the two sets to each other — it can see both crates, and this one cannot see
/// the catalogue at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedOperation {
    /// The model found nothing to do.
    ///
    /// It has a spelling of its own because it stopped being expressible any
    /// other way. While `install_module` was the only operation, an empty
    /// `targets` list said it: nothing to install is nothing to do. Most verbs
    /// of the catalogue take no arguments at all, so an empty list on `disks`
    /// is a complete request rather than an abstention, and one of the two
    /// meanings had to get a word.
    ///
    /// `Gamas-de-Modelo.md` calls abstention the measurement that matters most.
    /// A model that cannot decline invents on every ambiguous sentence, and it
    /// is not bad at abstaining — it has been made unable to.
    Nothing,

    List,

    Read,

    Go,

    Where,

    MakeDirectory,

    MakeFile,

    Copy,

    Move,

    Remove,

    Edit,

    Structured,

    Rehearse,

    Describe,

    IndexBuild,

    DependsOn,

    DependedOnBy,

    Symbol,

    Find,

    Grep,

    Processes,

    Memory,

    Stop,

    History,

    Attempt,

    Changes,

    Clear,

    /// Put the one screen on the display —
    /// `vault/02-Arquitectura/La-Pantalla.md`.
    ///
    /// Proposable because the catalogue is one surface and a verb missing from
    /// the grammar is a verb a model has no way to ask for. It is also the one
    /// verb whose answer is not text: it refuses on the structured face and on
    /// anything that is not a keyboard, so a model that names it has named a
    /// place to be rather than something done.
    Screen,

    Available,

    /// `install_module` deserialises to this too, and that alias is not a
    /// legacy spelling waiting to be tidied away.
    ///
    /// Every captured sample of a real model's output in
    /// `grammar_effect.rs` — including the one the 3B produced when it
    /// abstained, which is the only evidence that tier can — spells it that
    /// way, because that was the grammar's only word for eleven days. Rule 6
    /// of `Estrategia-de-Pruebas.md` is that a captured sample is the format
    /// and a rewritten one is your model of it, so the samples stay verbatim
    /// and the parser stays able to read them.
    ///
    /// The grammar emits `install`, so the alias is only ever the parser being
    /// broader than the grammar — which is the safe direction. The other way
    /// round is a grammar naming something the parser refuses, and that makes
    /// every inference fail while looking like a bad model.
    #[serde(alias = "install_module")]
    Install,

    Modules,

    Run,

    Permissions,

    /// Make the kernel guard binding.
    Deny,

    /// Stop the kernel guard from denying.
    ///
    /// Proposable, and that is the decree of 2026-08-24 working as written:
    /// the line is what the machine *does* without a human at a terminal, not
    /// what the model may *say*. The structured face refuses this one with
    /// `needs_a_human` no matter who asked.
    Observe,

    Rollback,

    State,

    Kernel,

    Disks,

    Network,

    /// Which keyboard layout the kernel holds —
    /// `crates/thalyx-term/src/keymap.rs`. A first-class operation and not a
    /// setting, because on a machine whose every sentence is Spanish it is the
    /// difference between writing and not.
    Keyboard,

    InstallOnto,

    /// Run a program nobody signed, confined —
    /// `vault/02-Arquitectura/Programas-Ajenos.md`.
    ///
    /// Proposable like the rest of the catalogue, and no weaker for it: the
    /// verb refuses on the structured face and asks a human at a terminal
    /// whatever proposed it. A model that names this has named a question, not
    /// an act.
    Execute,

    Leave,

    PowerOff,
}

impl ProposedOperation {
    /// The spelling this operation has on the wire.
    ///
    /// Exists so that [`crate::grammar`] can be built from it rather than
    /// repeating the literal. Two places holding the same judgement is how a
    /// grammar comes to permit a word the parser rejects, and a model
    /// constrained to say something unparseable fails on every utterance while
    /// looking, from the outside, like a model that is simply bad.
    ///
    /// The match is exhaustive on purpose: a variant added to the enum stops
    /// this compiling, which is the only guard that does not rely on somebody
    /// remembering.
    pub const fn name(self) -> &'static str {
        match self {
            ProposedOperation::Nothing => "nothing",
            ProposedOperation::List => "list",
            ProposedOperation::Read => "read",
            ProposedOperation::Go => "go",
            ProposedOperation::Where => "where",
            ProposedOperation::MakeDirectory => "make_directory",
            ProposedOperation::MakeFile => "make_file",
            ProposedOperation::Copy => "copy",
            ProposedOperation::Move => "move",
            ProposedOperation::Remove => "remove",
            ProposedOperation::Edit => "edit",
            ProposedOperation::Structured => "structured",
            ProposedOperation::Rehearse => "rehearse",
            ProposedOperation::Describe => "describe",
            ProposedOperation::IndexBuild => "index_build",
            ProposedOperation::DependsOn => "depends_on",
            ProposedOperation::DependedOnBy => "depended_on_by",
            ProposedOperation::Symbol => "symbol",
            ProposedOperation::Find => "find",
            ProposedOperation::Grep => "grep",
            ProposedOperation::Processes => "processes",
            ProposedOperation::Memory => "memory",
            ProposedOperation::Stop => "stop",
            ProposedOperation::History => "history",
            ProposedOperation::Attempt => "attempt",
            ProposedOperation::Changes => "changes",
            ProposedOperation::Clear => "clear",
            ProposedOperation::Screen => "screen",
            ProposedOperation::Available => "available",
            ProposedOperation::Install => "install",
            ProposedOperation::Modules => "modules",
            ProposedOperation::Run => "run",
            ProposedOperation::Permissions => "permissions",
            ProposedOperation::Deny => "deny",
            ProposedOperation::Observe => "observe",
            ProposedOperation::Rollback => "rollback",
            ProposedOperation::State => "state",
            ProposedOperation::Kernel => "kernel",
            ProposedOperation::Disks => "disks",
            ProposedOperation::Network => "network",
            ProposedOperation::Keyboard => "keyboard",
            ProposedOperation::InstallOnto => "install_onto",
            ProposedOperation::Execute => "execute",
            ProposedOperation::Leave => "leave",
            ProposedOperation::PowerOff => "power_off",
        }
    }

    /// The contract the core carries this out under, when there is one.
    ///
    /// [`None`] for most of the catalogue, and that is not a gap. A contract is
    /// the shape `vault/04-Flujo-Canonico/Contrato-Estructurado.md` gives to an
    /// operation that **changes the machine and needs a human to say yes**:
    /// provenance on every field, a rendered confirmation, a journal entry, a
    /// way back. Asking what is in a directory is not that, and dressing it as
    /// a contract would make the word mean nothing.
    ///
    /// So a proposal of `list` produces a [`crate::Plan::Verb`] instead, with
    /// its arguments attributed exactly the same way. What it does not produce
    /// is a contract claiming to be an install, which is what this returned
    /// before it existed: [`crate::assemble`] wrote `InstallModule` whatever
    /// the model had proposed, because there was only ever one thing to write.
    pub const fn contract_operation(self) -> Option<thalyx_contract::Operation> {
        match self {
            ProposedOperation::Install => Some(thalyx_contract::Operation::InstallModule),
            ProposedOperation::Remove => Some(thalyx_contract::Operation::DeleteFiles),
            ProposedOperation::IndexBuild => Some(thalyx_contract::Operation::BuildGraph),
            _ => None,
        }
    }

    /// Whether this operation's arguments are module ids rather than free text.
    ///
    /// It decides which target rule the grammar hands the model, and that is
    /// the one guarantee widening the catalogue would otherwise have cost.
    /// While `install_module` was the only operation, every target was an id
    /// and the grammar declined to spend tokens on anything that was not
    /// shaped like one. A single loose rule for all thirty-nine would have
    /// dropped that for the two operations it was protecting.
    ///
    /// `thalyx-cli` checks this against the catalogue's own `takes`, which is
    /// the authority on what a verb is given.
    pub const fn takes_module_id(self) -> bool {
        matches!(self, ProposedOperation::Install | ProposedOperation::Run)
    }

    /// Every operation the agent can propose.
    pub const ALL: [ProposedOperation; 45] = [
        ProposedOperation::Nothing,
        ProposedOperation::List,
        ProposedOperation::Read,
        ProposedOperation::Go,
        ProposedOperation::Where,
        ProposedOperation::MakeDirectory,
        ProposedOperation::MakeFile,
        ProposedOperation::Copy,
        ProposedOperation::Move,
        ProposedOperation::Remove,
        ProposedOperation::Edit,
        ProposedOperation::Structured,
        ProposedOperation::Rehearse,
        ProposedOperation::Describe,
        ProposedOperation::IndexBuild,
        ProposedOperation::DependsOn,
        ProposedOperation::DependedOnBy,
        ProposedOperation::Symbol,
        ProposedOperation::Find,
        ProposedOperation::Grep,
        ProposedOperation::Processes,
        ProposedOperation::Memory,
        ProposedOperation::Stop,
        ProposedOperation::History,
        ProposedOperation::Attempt,
        ProposedOperation::Changes,
        ProposedOperation::Clear,
        ProposedOperation::Screen,
        ProposedOperation::Available,
        ProposedOperation::Install,
        ProposedOperation::Modules,
        ProposedOperation::Run,
        ProposedOperation::Permissions,
        ProposedOperation::Deny,
        ProposedOperation::Observe,
        ProposedOperation::Rollback,
        ProposedOperation::State,
        ProposedOperation::Kernel,
        ProposedOperation::Disks,
        ProposedOperation::Network,
        ProposedOperation::Keyboard,
        ProposedOperation::InstallOnto,
        ProposedOperation::Execute,
        ProposedOperation::Leave,
        ProposedOperation::PowerOff,
    ];
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
    /// The completion at the front of a tool's output, without what the tool
    /// printed after it.
    ///
    /// ## The gap this closes
    ///
    /// Added 2026-08-08, after the first run that ever got as far as an answer.
    ///
    /// `prompt.rs`'s marker says where the answer **begins**. Nothing said where
    /// it **ends** — and llama.cpp's completion tool prints ` [end of text]`
    /// after the completion whenever the model stopped on an end-of-generation
    /// token. So Qwen emitted exactly the object the grammar describes, and
    /// Thalyx read object-plus-suffix, found it was not JSON, and reported the
    /// tool as ignoring a grammar it had just obeyed.
    ///
    /// ## Why the end comes from the grammar and not from that suffix
    ///
    /// `root` is one object, so the completion ends where the first complete
    /// JSON value ends and every byte after it was written by whatever printed
    /// it. Trimming the literal ` [end of text]` instead would be rule 6 the
    /// wrong way round: that string is one captured sample of one build's
    /// output, not the format. Whatever the next build appends, it is still
    /// after the object.
    ///
    /// [`None`] when nothing at the front is a JSON value at all — which, under
    /// a grammar, means the grammar was not applied.
    pub fn completion_in(raw: &str) -> Option<&str> {
        let start = raw.find(|c: char| !c.is_whitespace())?;
        let rest = &raw[start..];

        // Reading one value off a stream rather than matching braces: a brace
        // counter cannot tell `{` in a module id from `{` in the syntax, and a
        // target is a string the model chooses.
        let mut values = serde_json::Deserializer::from_str(rest).into_iter::<serde_json::Value>();
        values.next()?.ok()?;
        Some(&rest[..values.byte_offset()])
    }

    /// Read a model's raw output.
    ///
    /// Everything a model can do wrong ends here, and ends as an error rather
    /// than as a proposal with something missing.
    ///
    /// Strict about trailing text on purpose, and [`Proposal::completion_in`]
    /// is the one place allowed to be loose about it. Loosening this instead
    /// would mean every route into the core — fakes, other backends, the
    /// deterministic path — silently accepted bytes it has no reason to.
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

    /// Captured verbatim from a real run on 2026-08-08: llama.cpp `b1-3653e6d`,
    /// `llama-completion`, Qwen2.5-3B-Instruct-Q4_K_M, on Cesar's Fedora.
    ///
    /// Rule 6 asks for exactly one of these and this file had never had one.
    /// Every fixture above was written by the same person who wrote the parser,
    /// so every one of them agreed with the parser about where an answer stops
    /// — which is the single thing the parser had wrong.
    const CAPTURED: &str = r#"{
  "operation": "install_module",
  "targets": [
    "dev.thalyx.demo"
  ]
} [end of text]"#;

    #[test]
    fn the_answer_a_real_llama_cpp_gave_is_a_proposal() {
        // It was reported as a tool ignoring the grammar. It is a correct
        // answer with llama.cpp's own end-of-generation marker behind it.
        let completion =
            Proposal::completion_in(CAPTURED).expect("the object is at the front of it");
        let proposal = Proposal::parse(completion)
            .expect("a correct answer from a real model was refused as a broken tool");

        assert_eq!(proposal.operation, ProposedOperation::Install);
        assert_eq!(proposal.targets, ["dev.thalyx.demo"]);
        assert!(
            !completion.contains("end of text"),
            "the tool's suffix was left on the model's answer: {completion:?}"
        );
    }

    #[test]
    fn strict_parsing_still_refuses_the_suffix_the_tool_added() {
        // The control for the test above, and the reason the two functions are
        // separate. If `parse` had been loosened instead, this would pass by
        // accepting trailing text everywhere rather than at the one boundary
        // where another program is doing the printing.
        assert!(matches!(
            Proposal::parse(CAPTURED),
            Err(ProposalError::Malformed(_))
        ));
    }

    #[test]
    fn only_the_first_value_is_read_so_a_second_object_cannot_smuggle_anything() {
        // The grammar's root is one object, so a second one did not come from a
        // constrained decode. Reading past the first would let whatever printed
        // it choose the answer.
        let two = concat!(
            r#"{"operation": "install_module", "targets": ["dev.thalyx.demo"]}"#,
            "\n",
            r#"{"operation": "install_module", "targets": ["dev.evil.module"]}"#,
        );

        let proposal = Proposal::parse(Proposal::completion_in(two).unwrap()).unwrap();
        assert_eq!(proposal.targets, ["dev.thalyx.demo"]);
    }

    #[test]
    fn output_that_does_not_begin_with_a_value_has_no_completion_in_it() {
        // Rule 9. The loosened boundary must not become a scanner that goes
        // looking for an object somewhere in a page of prose — under a grammar
        // the completion starts at the front, and anything else is the grammar
        // not being applied.
        for raw in [
            "Sure! I can help you install that module.",
            "",
            "   \n\t ",
            "]",
            r#"I think {"operation": "install_module", "targets": []}"#,
        ] {
            assert_eq!(
                Proposal::completion_in(raw),
                None,
                "found a completion in {raw:?}"
            );
        }
    }

    #[test]
    fn a_well_formed_proposal_parses() {
        let proposal = Proposal::parse(
            r#"{"operation": "install_module", "targets": ["thalyx.demo"], "constraint": "^1.0"}"#,
        )
        .expect("this is the shape the grammar produces");

        assert_eq!(proposal.operation, ProposedOperation::Install);
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
