//! What the grammar changes about an answer, measured instead of argued.
//!
//! ## The question this exists to answer
//!
//! Six runs of `thalyx agent bench` across three model sizes reported the same
//! number for the measurement `vault/02-Arquitectura/Gamas-de-Modelo.md` calls
//! the most important one: **abstention, zero out of forty-six**. Not one
//! sentence that deserved "there is nothing to install here" got it, from a
//! 1.5B, a 3B or a 7B.
//!
//! A result that does not move when the only variable moves is evidence about
//! what those runs *share*, and the first line of the grammar is one of the
//! things they share:
//!
//! ```text
//! root      ::= "{" ws "\"operation\"" ws ":" ws operation ws "," ws "\"targets\"" …
//! operation ::= "\"install_module\""
//! ```
//!
//! [`crate::proposal::ProposedOperation::ALL`] has one element. The field order
//! is fixed. So **the first thing the model writes, in every inference, is
//! `install_module`** — it is given no alternative — and abstaining means
//! reaching `targets` and contradicting what it was just made to say. A model
//! that conditions on its own output has already told itself this is an install.
//!
//! That is a hypothesis with a mechanism, and it competes with others: that the
//! prompt's instruction to abstain carries too little weight, or that
//! Qwen2.5-Instruct is tuned to be helpful and "nothing to do" is the least
//! helpful thing there is. This module does not argue between them. It runs the
//! same prompt twice, with and without `--grammar-file`, and counts.
//!
//! ## Why this counts module ids instead of reading prose
//!
//! Without the grammar the answer is free text, and the temptation is to write
//! something that decides whether free text "declined". That would be a parser
//! for another tool's output built from fixtures its author invented, which is
//! the mistake `Estrategia-de-Pruebas.md` records twice — the second time
//! accusing llama.cpp of ignoring a grammar it had just obeyed.
//!
//! So nothing here interprets prose. It asks one mechanical question, the same
//! one [`crate::attribution::attribute`] asks and with the same authority behind
//! it: **did this answer name something that appears in nothing the model was
//! told?** Inventing is the failure under study, and inventing is countable.
//!
//! What the first run taught, at the cost of forty wasted inferences, is that
//! the free arm is mostly **not prose**. Take the grammar off and the 3B still
//! answers `instala algo bueno` with
//! `{"operation": "install_module", "targets": ["good-bad-thing"]}` — because
//! the *prompt* asks for JSON, and the prompt is in both arms. So both arms go
//! through [`what_an_answer_named`], which reads a proposal first and only
//! falls back to scanning when there is none. The version that scanned the free
//! arm as prose called that answer silence, which is the reading that says the
//! model declined.
//!
//! [`Named::Attributable`] is deliberately weak and says so: it means real ids
//! were named, not that they were the wrong answer. A proposal that names a
//! listed module in a sentence that asked for nothing is wrong and lands there,
//! and so does prose reading "no module matches; the available one is
//! dev.thalyx.demo", which is a decline. The contrast that carries weight is
//! between [`Named::Invented`] and [`Named::Nothing`]; the middle case is
//! printed for a human to read and never counted as either.
//!
//! ## The control, without which this proves nothing
//!
//! Rule 4: every denial test needs a baseline and a control. The baseline is the
//! constrained arm, which is what the bench already measures. The control is the
//! cases where a module **should** be named.
//!
//! Without it, "the free arm named nothing" has two readings that look
//! identical — the model declined, or the model rambles when no grammar is
//! shaping it and never names anything at all. So the acting cases are run both
//! ways too, and if the free arm cannot name the right module even where there
//! is one, [`Effect::Inconclusive`] is the answer and no verdict is given.
//! A probe that cannot fail is not a probe.

use crate::attribution::attribute;
use crate::proposal::Proposal;
use crate::router::looks_like_module_id;
use crate::transcript::Transcript;

/// What llama.cpp prints when the model ends generation, captured verbatim from
/// the light tier on 2026-08-08.
///
/// Recognised in exactly one place — telling "generated no content" apart from
/// "said something that named nothing" — and nowhere near the parser, which is
/// right to refuse to know it. One captured sample of one build's output is not
/// the format, so this is a hint and never a guarantee: the thing that actually
/// protects the verdict when a model goes quiet is the control.
const END_OF_GENERATION: &str = "[end of text]";

/// What a piece of text did about naming a module, decided mechanically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Named {
    /// The model generated no content at all.
    ///
    /// Kept apart from [`Named::Nothing`], and the first run of this probe is
    /// why. Without a grammar the light tier answered **every one of twenty
    /// cases** with an immediate end of generation, and folding that into
    /// "named nothing" would have called it a tier that declines — the same
    /// mistake, on the same model, that put a `PROVEN` under the grammar probe
    /// two days earlier. A model that emitted zero tokens did not decline. It
    /// did not answer.
    SaidNothingAtAll,
    /// Said something, and no module id was in it.
    Nothing,
    /// Named only ids that appear in what the model was told.
    ///
    /// **Not the same as proposing to install them.** Prose that lists what is
    /// available in order to decline lands here. Counted as neither invention
    /// nor silence; printed so a human decides.
    Attributable(String),
    /// Named an id that appears in nothing the model was told.
    ///
    /// This is the failure the abstention cases produce under the grammar, and
    /// the one thing here that needs no interpretation.
    Invented(String),
}

/// Whether `c` could be part of a module id, for cutting text into candidates.
///
/// Alphanumerics rather than lowercase-only, on purpose: `dev.thalyx.demoX` is
/// not an id, and a splitter that stopped at the `X` would hand
/// `dev.thalyx.demo` over as if the model had named the real thing. Keeping the
/// `X` inside the run lets [`looks_like_module_id`] reject the whole of it.
fn could_be_inside_an_id(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'
}

/// Every module id in `text`, in the order they appear.
///
/// [`looks_like_module_id`] stays the authority on **what an id is**; what
/// differs here is only how the text is cut into candidates, and it differs for
/// a reason worth stating. The router tokenises on whitespace because it reads
/// what a human typed. This reads what a *model* emitted, which is JSON at
/// least half the time — `["dev.thalyx.demo"]` is one whitespace token, and the
/// first version of this function found nothing in it.
///
/// That mattered in the dangerous direction. A scanner blind to JSON reports
/// [`Named::Nothing`] for an answer full of inventions, and `Nothing` is one of
/// the two readings this whole module exists to tell apart — it would have
/// reported the grammar guilty on the strength of not being able to read the
/// evidence.
pub fn module_ids_in(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();

    for run in text.split(|c: char| !could_be_inside_an_id(c)) {
        // A sentence ending in an id leaves a trailing dot on the run, and a
        // trailing dot makes an empty last segment, which is not an id.
        let candidate = run.trim_matches('.');
        if looks_like_module_id(candidate) && !found.iter().any(|seen| seen == candidate) {
            found.push(candidate.to_string());
        }
    }
    found
}

/// What a parsed proposal named, judged by the rule [`what_it_named`] uses.
///
/// The constrained arm arrives as a contract, so its targets are read by the
/// parser rather than scanned out of text — same question, right authority for
/// the shape in hand.
pub fn what_a_proposal_named(targets: &[String], transcript: &Transcript) -> Named {
    if let Some(invented) = targets
        .iter()
        .find(|id| attribute(id, transcript).is_err())
        .cloned()
    {
        return Named::Invented(invented);
    }

    match targets.first() {
        Some(real) => Named::Attributable(real.clone()),
        None => Named::Nothing,
    }
}

/// What one arm's raw output did about naming a module.
///
/// The entry point both arms go through, and it exists because the first
/// version of this probe did not have it. That version judged the constrained
/// arm with [`Proposal::parse`] — which is strict about trailing text on
/// purpose — so every constrained answer in a forty-inference run came back
/// `NO MEASUREMENT: trailing characters`, because llama.cpp appends its own
/// end-of-generation notice after the object. [`Proposal::completion_in`] is
/// the function that already knew this, and the bench had been using it all
/// along.
///
/// The second defect was worse, because it produced a *reading* rather than an
/// error. The free arm was scanned for module ids as prose, and without a
/// grammar the 3B answers with JSON naming things that are not module ids:
///
/// ```text
/// {"operation": "install_module", "targets": ["good-bad-thing"]}
/// {"operation": "install_module", "targets": ["github.com/example/module1"]}
/// ```
///
/// A scanner looking for reverse-DNS ids finds none of that and reports
/// [`Named::Nothing`] — **silence** — for an answer that proposed installing
/// two invented things. Three different facts arrived as one string, and the
/// one they were collapsed into is the answer this probe exists to detect.
///
/// So: read a proposal out of either arm first and judge its targets, whatever
/// shape those targets are in. Only when there is no proposal at all is the
/// text prose, and only then is it scanned.
pub fn what_an_answer_named(raw: &str, transcript: &Transcript) -> Named {
    let said = raw.trim().replace(END_OF_GENERATION, "");
    if said.trim().is_empty() {
        return Named::SaidNothingAtAll;
    }

    if let Some(value) = Proposal::completion_in(raw)
        && let Ok(proposal) = Proposal::parse(value)
    {
        return what_a_proposal_named(&proposal.targets, transcript);
    }

    what_it_named(raw, transcript)
}

/// What `text` named, judged against what the model was told.
///
/// Prose only — see [`what_an_answer_named`], which is what callers want.
///
/// An invention anywhere outweighs anything attributable: naming one thing
/// nobody mentioned is the failure being measured, and an answer that also
/// mentioned a real module did not stop inventing by doing so.
pub fn what_it_named(text: &str, transcript: &Transcript) -> Named {
    let ids = module_ids_in(text);

    if let Some(invented) = ids
        .iter()
        .find(|id| attribute(id, transcript).is_err())
        .cloned()
    {
        return Named::Invented(invented);
    }

    match ids.into_iter().next() {
        Some(real) => Named::Attributable(real),
        None => Named::Nothing,
    }
}

/// One case, measured both ways.
#[derive(Debug, Clone)]
pub struct BothArms {
    /// What the answer with `--grammar-file` named, or the failure that stopped
    /// it. Never inferred from the other arm.
    pub constrained: Result<Named, String>,
    pub unconstrained: Result<Named, String>,
    /// Whether declining was the right answer for this case.
    pub wants_abstention: bool,
    /// The id a correct answer would have named, when there is one.
    pub expected: Option<String>,
}

/// What the two arms did across a whole suite.
///
/// Every field is a count of things that were measured. A case whose arm failed
/// is in no count at all, so nothing here is ever inflated by a run that did not
/// happen — the mistake that once let a model which never answered score full
/// marks on abstention.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    /// Abstention cases where the constrained arm invented an id.
    pub abstention_constrained_invented: usize,
    /// Abstention cases where the free arm invented an id.
    pub abstention_free_invented: usize,
    /// Abstention cases where the free arm said something naming no module.
    pub abstention_free_silent: usize,
    /// Abstention cases where the free arm generated no content at all.
    ///
    /// Its own column and in no total, because it is not an answer. Folded into
    /// `abstention_free_silent` it would read as a tier that declines, which is
    /// what the light tier would have been called on this probe's first run.
    pub abstention_free_said_nothing_at_all: usize,
    /// Abstention cases where the free arm named only real ids. Neither column.
    pub abstention_free_attributable: usize,
    /// Acting cases where both arms were measured.
    pub acting_measured: usize,
    /// Acting cases where the free arm named the id a correct answer wanted.
    pub acting_free_named_expected: usize,
}

impl Tally {
    /// Count one case that has been measured both ways.
    pub fn count(&mut self, arms: &BothArms) {
        let (Ok(constrained), Ok(free)) = (&arms.constrained, &arms.unconstrained) else {
            // One arm without the other answers nothing: the whole question is
            // a comparison. Rule 10 — this is a failure to read, not a result.
            return;
        };

        if arms.wants_abstention {
            if matches!(constrained, Named::Invented(_)) {
                self.abstention_constrained_invented += 1;
            }
            match free {
                Named::Invented(_) => self.abstention_free_invented += 1,
                Named::Nothing => self.abstention_free_silent += 1,
                Named::Attributable(_) => self.abstention_free_attributable += 1,
                Named::SaidNothingAtAll => self.abstention_free_said_nothing_at_all += 1,
            }
            return;
        }

        self.acting_measured += 1;
        if let (Some(expected), Named::Attributable(named)) = (&arms.expected, free)
            && named == expected
        {
            self.acting_free_named_expected += 1;
        }
    }

    /// Abstention cases where both arms produced something to compare.
    ///
    /// A free arm that generated no content is deliberately not in here: there
    /// is nothing to set beside the constrained arm.
    pub fn abstention_measured(&self) -> usize {
        self.abstention_free_invented
            + self.abstention_free_silent
            + self.abstention_free_attributable
    }

    /// Whether the free arm works well enough for its silence to mean anything.
    ///
    /// Half of the acting cases, and the threshold is a judgement worth naming:
    /// below half, the free arm gets the easy direction wrong more often than
    /// right, and an instrument that is usually wrong cannot be read as
    /// deliberate when it says nothing.
    pub fn control_holds(&self) -> bool {
        self.acting_measured > 0 && self.acting_free_named_expected * 2 >= self.acting_measured
    }

    /// What the two arms, taken together, are evidence for.
    pub fn verdict(&self) -> Effect {
        if self.abstention_measured() == 0 && self.abstention_free_said_nothing_at_all > 0 {
            return Effect::Inconclusive {
                why: "without the grammar the model generated no content at all on \
                      every abstention case, and a model that emitted no tokens did \
                      not decline — it did not answer",
            };
        }

        if self.abstention_measured() == 0 {
            return Effect::Inconclusive {
                why: "no abstention case was measured on both arms, so there is \
                      nothing here to compare",
            };
        }

        if !self.control_holds() {
            return Effect::Inconclusive {
                why: "without the grammar the model did not name the right module \
                      even where there was one, so its silence on the abstention \
                      cases is not a decision to decline",
            };
        }

        if self.abstention_constrained_invented == 0 {
            return Effect::Inconclusive {
                why: "the constrained arm did not invent on these cases either, \
                      so there is no failure here for the grammar to explain",
            };
        }

        if self.abstention_free_invented == 0 {
            Effect::GrammarTakesTheDecision
        } else {
            Effect::InventsEitherWay
        }
    }
}

/// What the paired run showed about the grammar's part in abstention failing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Constrained it invented; left alone it never did. The grammar is what
    /// stands between this model and declining.
    GrammarTakesTheDecision,
    /// It invented with the grammar and without it. Whatever is wrong is not
    /// the grammar, and the hypothesis is refuted for these cases.
    InventsEitherWay,
    /// The two arms could not be told apart, and this says which way.
    Inconclusive { why: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Segment;

    fn told(text: &str) -> Transcript {
        Transcript::new().with(Segment::typed(text))
    }

    fn arms(constrained: Named, free: Named, wants_abstention: bool) -> BothArms {
        BothArms {
            constrained: Ok(constrained),
            unconstrained: Ok(free),
            wants_abstention,
            expected: Some("dev.thalyx.demo".to_string()),
        }
    }

    #[test]
    fn an_id_nobody_mentioned_is_an_invention_however_well_formed_it_is() {
        let transcript = told("available: dev.thalyx.demo 1.4.2");
        assert_eq!(
            what_it_named("you want org.openjdk.jmh", &transcript),
            Named::Invented("org.openjdk.jmh".to_string())
        );
    }

    #[test]
    fn a_real_id_extended_by_one_segment_is_a_different_id_and_an_invention() {
        // The light tier's actual answer to case 4, and the reason this cannot
        // be a substring check: `dev.thalyx.demo.versions` contains a real id
        // and is not one.
        let transcript = told("available: dev.thalyx.demo 1.4.2, 2.0.0");
        assert_eq!(
            what_it_named(r#"["dev.thalyx.demo.versions.versions"]"#, &transcript),
            Named::Invented("dev.thalyx.demo.versions.versions".to_string())
        );
    }

    #[test]
    fn prose_that_names_nothing_is_silence_and_not_an_invention() {
        let transcript = told("instala algo bueno");
        assert_eq!(
            what_it_named("I could not find a module matching that.", &transcript),
            Named::Nothing
        );
    }

    #[test]
    fn one_invention_outweighs_any_number_of_real_ids_beside_it() {
        // An answer that also mentioned something real did not stop inventing
        // by doing so, and counting it as attributable would hide the failure
        // under the evidence that it was nearly right.
        let transcript = told("available: dev.thalyx.demo");
        assert_eq!(
            what_it_named("maybe dev.thalyx.demo, or com.adobe.photoshop", &transcript),
            Named::Invented("com.adobe.photoshop".to_string())
        );
    }

    #[test]
    fn a_free_arm_that_names_nothing_anywhere_proves_nothing_about_abstention() {
        // Rule 4, and the whole reason the acting cases are run at all. A model
        // that never names a module without a grammar is silent on the
        // abstention cases for a reason that has nothing to do with declining,
        // and without this the run would report the grammar guilty.
        let mut tally = Tally::default();
        for _ in 0..4 {
            tally.count(&arms(
                Named::Invented("org.openjdk.jmh".to_string()),
                Named::Nothing,
                true,
            ));
        }
        for _ in 0..4 {
            tally.count(&arms(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Nothing,
                false,
            ));
        }

        assert!(!tally.control_holds());
        assert!(
            matches!(tally.verdict(), Effect::Inconclusive { .. }),
            "got {:?}",
            tally.verdict()
        );
    }

    #[test]
    fn a_free_arm_that_still_finds_the_right_module_makes_its_silence_mean_something() {
        let mut tally = Tally::default();
        for _ in 0..4 {
            tally.count(&arms(
                Named::Invented("org.openjdk.jmh".to_string()),
                Named::Nothing,
                true,
            ));
        }
        for _ in 0..4 {
            tally.count(&arms(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                false,
            ));
        }

        assert!(tally.control_holds());
        assert_eq!(tally.verdict(), Effect::GrammarTakesTheDecision);
    }

    #[test]
    fn inventing_on_both_arms_refutes_the_hypothesis_rather_than_confirming_it() {
        let mut tally = Tally::default();
        tally.count(&arms(
            Named::Invented("org.openjdk.jmh".to_string()),
            Named::Invented("com.adobe.photoshop".to_string()),
            true,
        ));
        for _ in 0..2 {
            tally.count(&arms(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                false,
            ));
        }

        assert_eq!(tally.verdict(), Effect::InventsEitherWay);
    }

    #[test]
    fn a_case_whose_arm_failed_is_counted_in_nothing() {
        // The defect this forestalls is the one the bench already had once: a
        // run that never happened counted as a result. Here it would be worse,
        // because a missing free arm would read as silence, and silence is one
        // of the two answers.
        let mut tally = Tally::default();
        tally.count(&BothArms {
            constrained: Ok(Named::Invented("org.openjdk.jmh".to_string())),
            unconstrained: Err("the model ran out of tokens".to_string()),
            wants_abstention: true,
            expected: None,
        });

        assert_eq!(tally, Tally::default());
        assert_eq!(tally.abstention_measured(), 0);
        assert!(matches!(tally.verdict(), Effect::Inconclusive { .. }));
    }

    #[test]
    fn a_grammar_that_never_made_it_invent_has_nothing_to_be_guilty_of() {
        // Without this the run would report GrammarTakesTheDecision on a suite
        // where the constrained arm abstained correctly — a verdict about a
        // failure that did not occur.
        let mut tally = Tally::default();
        tally.count(&arms(Named::Nothing, Named::Nothing, true));
        for _ in 0..2 {
            tally.count(&arms(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                false,
            ));
        }

        assert!(matches!(tally.verdict(), Effect::Inconclusive { .. }));
    }

    /// Captured verbatim from the first run of this probe, 2026-08-08, on
    /// Cesar's Fedora: llama.cpp `b1-3653e6d`, Qwen2.5-3B-Instruct-Q4_K_M, the
    /// **free** arm answering `instala algo bueno` — a case whose right answer
    /// is to decline.
    ///
    /// Rule 6, and this module had none when it was written. Every fixture
    /// above was invented by the same person who wrote the scanner, so every
    /// one of them agreed with the scanner about what a model's free answer
    /// looks like. It looks like this, and the scanner read it as silence.
    const FREE_ARM_PROPOSING_A_NON_ID: &str = concat!(
        r#"{"operation": "install_module", "targets": ["good-bad-thing"]} "#,
        "<<<THALYX-0e4763b88ae142a28e3348ee4b3d3d22>>> ",
        r#"{"operation": "install_module", "targets": ["good-bad-thing"], "constraint": "^1.0"}"#
    );

    /// The same run, the same arm, answering `ese, el que te dije` with things
    /// listed — targets that are URLs rather than ids.
    const FREE_ARM_PROPOSING_URLS: &str = concat!(
        r#"{"operation": "install_module", "targets": ["github.com/example/module1", "#,
        r#""github.com/example/module2"]} "#,
        "<<<THALYX-7edeab16f4734e1ca2ac9d7614011526>>>"
    );

    /// The constrained arm of that same run — every one of forty inferences
    /// ended like this, and [`Proposal::parse`] called all of them malformed.
    const CONSTRAINED_ARM_WITH_THE_TOOLS_TRAILER: &str =
        r#"{"operation": "install_module", "targets": ["dev.thalyx.demo"]} [end of text]"#;

    /// The light tier's free arm, on all twenty cases, in that same run.
    const FREE_ARM_THAT_GENERATED_NOTHING: &str = " [end of text]";

    #[test]
    fn a_target_that_is_not_an_id_is_still_something_it_proposed_installing() {
        // The defect this replaced would have decided the experiment. Scanned
        // as prose, `good-bad-thing` is not a reverse-DNS id, so the answer
        // read as `Nothing` — silence — which is the reading that says the
        // model declined. It proposed installing something nobody mentioned.
        let transcript = told("instala algo bueno");
        assert_eq!(
            what_an_answer_named(FREE_ARM_PROPOSING_A_NON_ID, &transcript),
            Named::Invented("good-bad-thing".to_string())
        );
        assert_eq!(
            what_an_answer_named(FREE_ARM_PROPOSING_URLS, &told("ese, el que te dije")),
            Named::Invented("github.com/example/module1".to_string())
        );
    }

    #[test]
    fn the_tools_own_trailer_after_an_object_is_not_a_malformed_proposal() {
        // `Proposal::parse` is strict about trailing text on purpose and this
        // probe called it directly, so all forty constrained inferences of the
        // first run came back NO MEASUREMENT and the run had nothing to
        // compare. `completion_in` is the function that already knew, and the
        // bench had been using it all along.
        let transcript = told("available: dev.thalyx.demo");
        assert_eq!(
            what_an_answer_named(CONSTRAINED_ARM_WITH_THE_TOOLS_TRAILER, &transcript),
            Named::Attributable("dev.thalyx.demo".to_string())
        );
    }

    #[test]
    fn a_model_that_generated_no_tokens_did_not_decline() {
        // The light tier answered every free-arm case with this. Counted as
        // `Nothing` it would have read as twenty declines in a row and made
        // this probe report the grammar guilty on a tier that said nothing at
        // all — which is exactly the shape of the `PROVEN` retired two days
        // before, on the same model, for the same reason.
        assert_eq!(
            what_an_answer_named(FREE_ARM_THAT_GENERATED_NOTHING, &told("instala algo bueno")),
            Named::SaidNothingAtAll
        );
        assert_eq!(
            what_an_answer_named("   ", &told("instala algo bueno")),
            Named::SaidNothingAtAll
        );
    }

    #[test]
    fn prose_is_only_scanned_when_there_is_no_proposal_to_read() {
        // Free answers carry a proposal and then a paragraph explaining it, and
        // the paragraph names modules the proposal did not choose. Judging the
        // whole text would credit the model with a target it never proposed.
        let transcript = told("available: dev.thalyx.demo, dev.thalyx.greeter");
        let answer = concat!(
            r#"{"operation": "install_module", "targets": ["dev.thalyx.demo"]} "#,
            "The other one, dev.thalyx.other, was ruled out."
        );
        assert_eq!(
            what_an_answer_named(answer, &transcript),
            Named::Attributable("dev.thalyx.demo".to_string()),
            "the explanation was read as the proposal"
        );
    }

    #[test]
    fn a_free_arm_that_only_ever_goes_quiet_is_not_a_tier_that_declines() {
        // The light tier's whole run, in one assertion. Every arm said nothing,
        // so nothing was measured and the control could not hold either.
        let mut tally = Tally::default();
        for wants_abstention in [true, true, true, false, false, false] {
            tally.count(&arms(
                Named::Invented("org.openjdk.jmh".to_string()),
                Named::SaidNothingAtAll,
                wants_abstention,
            ));
        }

        assert_eq!(tally.abstention_free_said_nothing_at_all, 3);
        assert_eq!(
            tally.abstention_measured(),
            0,
            "silence is not a measurement"
        );
        assert!(!tally.control_holds());
        assert!(
            matches!(tally.verdict(), Effect::Inconclusive { .. }),
            "got {:?}",
            tally.verdict()
        );
    }

    #[test]
    fn an_id_inside_json_is_found_because_that_is_what_the_grammar_produces() {
        // The defect this replaced, found by a test rather than by a run, and
        // it would have been invisible in the results: the constrained arm is
        // always JSON, so a scanner that reads `["dev.thalyx.demo"]` as one
        // unparseable token reports an answer full of inventions as silence.
        assert_eq!(
            module_ids_in(r#"{"operation": "install_module", "targets": ["dev.thalyx.demo"]}"#),
            ["dev.thalyx.demo"]
        );
    }

    #[test]
    fn an_id_with_something_stuck_to_it_is_not_that_id() {
        // A splitter that stopped at the capital would hand over the real id
        // and report the model as having named the right thing.
        assert!(module_ids_in("dev.thalyx.demoX").is_empty());
        assert_eq!(
            module_ids_in("install dev.thalyx.demo."),
            ["dev.thalyx.demo"]
        );
    }

    #[test]
    fn a_proposal_is_judged_by_its_targets_and_not_by_its_text() {
        let transcript = told("available: dev.thalyx.demo");
        assert_eq!(
            what_a_proposal_named(&["dev.thalyx.demo".to_string()], &transcript),
            Named::Attributable("dev.thalyx.demo".to_string())
        );
        assert_eq!(
            what_a_proposal_named(&[], &transcript),
            Named::Nothing,
            "an empty target list is how a proposal abstains"
        );
        assert_eq!(
            what_a_proposal_named(&["org.openjdk.jmh".to_string()], &transcript),
            Named::Invented("org.openjdk.jmh".to_string())
        );
    }

    #[test]
    fn every_id_in_a_text_is_found_and_not_only_the_first() {
        let ids = module_ids_in("try dev.thalyx.demo or dev.thalyx.greeter, not org.a.b");
        assert_eq!(ids, ["dev.thalyx.demo", "dev.thalyx.greeter", "org.a.b"]);
    }

    #[test]
    fn a_repeated_id_is_named_once() {
        let ids = module_ids_in("dev.thalyx.demo dev.thalyx.demo dev.thalyx.demo");
        assert_eq!(ids, ["dev.thalyx.demo"]);
    }

    #[test]
    fn a_text_that_names_nothing_terminates() {
        // `module_ids_in` walks by cutting the string past each hit. A hit it
        // could not locate would leave the string unchanged and spin forever,
        // so the empty and the no-match cases are the ones worth pinning.
        assert!(module_ids_in("").is_empty());
        assert!(module_ids_in("nothing here looks like one").is_empty());
    }
}
