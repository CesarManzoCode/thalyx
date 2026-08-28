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
//! ## What it answered, and the question that left — 2026-08-09
//!
//! **Refuted, on the 3B.** `IT INVENTS EITHER WAY`, with the control holding at
//! 9 of 11: with no grammar at all it still invented on four of the nine
//! abstention cases and named the wrong real module on five. Taking the
//! constraint away did not make it decline, so the constraint is not what stops
//! it. The hypothesis above is kept in full because it had a mechanism and
//! because writing it down this way is what made the experiment buildable.
//!
//! And the two arms could not go on to name the culprit, for a reason that was
//! in them from the start: **[`crate::prompt::Prompt::render`] asks for a JSON
//! object whose first field is an operation, and it is in both of them.** The
//! "free" arm was never free of that instruction — the two differed in who did
//! the forcing, not in whether forcing happened. So the remaining suspects,
//! *the prompt makes it propose* and *this family would rather help than
//! decline*, stayed indistinguishable, and no further run of the same pair
//! could separate them.
//!
//! Hence a **third arm**, [`crate::prompt::Prompt::in_prose`]: no object, no
//! operation named, one question and a word to decline with. Sixty inferences a
//! run instead of forty. It carries its own control and its own verdict —
//! [`Tally::prompt_verdict`] — rather than replacing [`Tally::verdict`], which
//! stays comparable with the runs that already exist.
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
//! The second successful run taught the same lesson a third time, and this one
//! cost the answer rather than the run. Under the grammar, asked `instala algo
//! bueno`, the 3B emitted `{"operation": "install_module", "targets": []}` —
//! **an abstention**, the behaviour six bench runs had reported as zero out of
//! forty-six — and this module printed it as "said something, named nothing",
//! which is also what it prints for a paragraph that rambled. The observation
//! the investigation existed to find was written down as noise. Hence
//! [`Named::Abstained`], which is a decision and never shares a word with an
//! omission.
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
use crate::prompt::ABSTENTION_WORD;
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
    /// Emitted a well-formed proposal whose target list was **empty**.
    ///
    /// This is not silence. It is the sentence `prompt.rs` teaches — now
    /// *answer with the operation nothing and an empty list of targets* — and the one
    /// `grammar.rs` widened the target rule to make expressible. It reaches the
    /// agent as [`crate::AgentError::NothingToDo`]. It is, in other words, the
    /// behaviour six bench runs reported as zero out of forty-six.
    ///
    /// Kept apart from [`Named::Nothing`] for the third time in this module,
    /// and the reason is the same one both times before: the first version of
    /// this enum folded it in, so a model that abstained and a model that
    /// rambled without naming anything printed the same words. The difference
    /// is the entire question.
    Abstained,
    /// Said something, and no module id was in it.
    ///
    /// Prose only. A proposal that named nothing is [`Named::Abstained`],
    /// because a proposal saying nothing is a proposal that decided.
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
        // An empty list is the one thing a proposal can say that is a decision
        // rather than an omission, and it has its own name so it can never be
        // reported as the model having gone quiet.
        None => Named::Abstained,
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

/// What the prose arm's raw output did, including whether it declined.
///
/// [`what_an_answer_named`] plus one rule, and the rule is deliberately narrow:
/// an answer that named no module **and** contains [`ABSTENTION_WORD`] exactly
/// is an abstention. Nothing else about the text is read.
///
/// Naming still outranks declining, and that ordering is not arbitrary. A model
/// answering "NOTHING matches exactly, but dev.thalyx.demo is close" did not
/// decline — it proposed, with a hedge in front — and counting the hedge would
/// score a proposal as a refusal. Same precedence [`what_it_named`] uses for
/// inventions, for the same reason.
///
/// The exact-case match is the conservative half, and its bias is stated on
/// [`ABSTENTION_WORD`]: a model declining in its own words falls to
/// [`Named::Nothing`] instead, which under-counts abstention in the direction
/// that blames the model. That column is printed rather than folded.
pub fn what_prose_named(raw: &str, transcript: &Transcript) -> Named {
    match what_an_answer_named(raw, transcript) {
        Named::Nothing if raw.contains(ABSTENTION_WORD) => Named::Abstained,
        otherwise => otherwise,
    }
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

/// One case, measured three ways.
#[derive(Debug, Clone)]
pub struct ThreeArms {
    /// What the answer with `--grammar-file` named, or the failure that stopped
    /// it. Never inferred from another arm.
    pub constrained: Result<Named, String>,
    /// The same prompt with the flag removed.
    pub unconstrained: Result<Named, String>,
    /// [`crate::prompt::Prompt::in_prose`]: no object, no operation named.
    ///
    /// Its own `Result` like the others. An arm that failed is in no count, and
    /// its failure is never read as the model having said nothing.
    pub prose: Result<Named, String>,
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
    /// Abstention cases where **both** arms produced an answer of some kind.
    ///
    /// The denominator the constrained column is counted over, and it is a
    /// field rather than a derived sum because the first version had no such
    /// thing: the constrained counts were printed under
    /// [`Tally::abstention_measured`], which excludes the cases where the free
    /// arm generated nothing, and the light tier's run therefore reported
    /// *invented 6* directly beneath *measured 2*. Six out of two is not a
    /// reading anyone can act on.
    pub abstention_paired: usize,
    /// Abstention cases where the constrained arm invented an id.
    pub abstention_constrained_invented: usize,
    /// Abstention cases where the constrained arm left the target list empty.
    ///
    /// The behaviour under study, on the arm that is actually shipped.
    pub abstention_constrained_abstained: usize,
    /// Abstention cases where the free arm invented an id.
    pub abstention_free_invented: usize,
    /// Abstention cases where the free arm left the target list empty.
    ///
    /// Kept apart from `abstention_free_silent` on the same grounds as
    /// [`Named::Abstained`]: a model that answered with an empty proposal chose
    /// to, and a model that wrote a paragraph naming nothing may not have.
    pub abstention_free_abstained: usize,
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
    /// Abstention cases where the prose arm invented an id.
    pub abstention_prose_invented: usize,
    /// Abstention cases where the prose arm said [`ABSTENTION_WORD`].
    pub abstention_prose_abstained: usize,
    /// Abstention cases where the prose arm said something that was neither.
    ///
    /// The column that carries this arm's known bias. A model declining in its
    /// own words instead of with the token lands here, and that under-counts
    /// abstention in the direction that blames the model — so it is printed,
    /// never folded, and never counted as a refusal to decline.
    pub abstention_prose_silent: usize,
    /// Abstention cases where the prose arm generated no content at all.
    pub abstention_prose_said_nothing_at_all: usize,
    /// Abstention cases where the prose arm named only real ids.
    pub abstention_prose_attributable: usize,
    /// Acting cases where both object arms were measured.
    pub acting_measured: usize,
    /// Acting cases where the free arm named the id a correct answer wanted.
    pub acting_free_named_expected: usize,
    /// Acting cases where the prose arm was measured.
    ///
    /// Counted apart from `acting_measured` because the prose arm can fail on
    /// its own, and a control counted over cases it never ran is not a control.
    pub acting_prose_measured: usize,
    /// Acting cases where the prose arm named the id a correct answer wanted.
    pub acting_prose_named_expected: usize,
}

impl Tally {
    /// Count one case that has been run three ways.
    ///
    /// The two questions are counted **separately**, because they fail
    /// separately. A prose arm that ran out of tokens says nothing about
    /// whether the grammar changed the object arms, and letting it discard that
    /// comparison would throw away a measurement that happened.
    pub fn count(&mut self, arms: &ThreeArms) {
        self.count_the_object_arms(arms);
        self.count_the_prose_arm(arms);
    }

    fn count_the_object_arms(&mut self, arms: &ThreeArms) {
        let (Ok(constrained), Ok(free)) = (&arms.constrained, &arms.unconstrained) else {
            // One arm without the other answers nothing: the whole question is
            // a comparison. Rule 10 — this is a failure to read, not a result.
            return;
        };

        if arms.wants_abstention {
            self.abstention_paired += 1;
            match constrained {
                Named::Invented(_) => self.abstention_constrained_invented += 1,
                Named::Abstained => self.abstention_constrained_abstained += 1,
                _ => {}
            }
            match free {
                Named::Invented(_) => self.abstention_free_invented += 1,
                Named::Abstained => self.abstention_free_abstained += 1,
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

    fn count_the_prose_arm(&mut self, arms: &ThreeArms) {
        let Ok(prose) = &arms.prose else {
            return;
        };

        if arms.wants_abstention {
            match prose {
                Named::Invented(_) => self.abstention_prose_invented += 1,
                Named::Abstained => self.abstention_prose_abstained += 1,
                Named::Nothing => self.abstention_prose_silent += 1,
                Named::Attributable(_) => self.abstention_prose_attributable += 1,
                Named::SaidNothingAtAll => self.abstention_prose_said_nothing_at_all += 1,
            }
            return;
        }

        self.acting_prose_measured += 1;
        if let (Some(expected), Named::Attributable(named)) = (&arms.expected, prose)
            && named == expected
        {
            self.acting_prose_named_expected += 1;
        }
    }

    /// Abstention cases where both arms produced something to compare.
    ///
    /// A free arm that generated no content is deliberately not in here: there
    /// is nothing to set beside the constrained arm.
    pub fn abstention_measured(&self) -> usize {
        self.abstention_free_invented
            + self.abstention_free_abstained
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

    /// Abstention cases where the prose arm produced an answer of some kind.
    pub fn prose_measured(&self) -> usize {
        self.abstention_prose_invented
            + self.abstention_prose_abstained
            + self.abstention_prose_silent
            + self.abstention_prose_attributable
    }

    /// Whether the prose arm works well enough for its declining to mean
    /// anything.
    ///
    /// Its own control, on its own denominator. Sharing
    /// [`Tally::control_holds`] would let an arm that never ran borrow the
    /// credibility of one that did.
    pub fn prose_control_holds(&self) -> bool {
        self.acting_prose_measured > 0
            && self.acting_prose_named_expected * 2 >= self.acting_prose_measured
    }

    /// What the prose arm, set against the two that ask for an object, says
    /// about whose decision the abstention is.
    ///
    /// A second verdict rather than a replacement. [`Tally::verdict`] answers
    /// the grammar question and stays comparable with the runs that already
    /// exist; this answers the one those runs raised.
    pub fn prompt_verdict(&self) -> PromptEffect {
        if self.prose_measured() == 0 && self.abstention_prose_said_nothing_at_all > 0 {
            return PromptEffect::Inconclusive {
                why: "asked in prose the model generated no content at all on every \
                      abstention case, and a model that emitted no tokens did not \
                      decline — it did not answer",
            };
        }

        if self.prose_measured() == 0 {
            return PromptEffect::Inconclusive {
                why: "the prose arm produced nothing to count on the abstention \
                      cases",
            };
        }

        if !self.prose_control_holds() {
            return PromptEffect::Inconclusive {
                why: "asked in prose the model did not name the right module even \
                      where there was one, so what it does on the abstention cases \
                      is not a decision about them",
            };
        }

        // Without a failure on the arms that ask for an object there is nothing
        // for the prompt to be blamed for, however the prose arm behaves.
        if self.abstention_constrained_invented == 0 && self.abstention_free_invented == 0 {
            return PromptEffect::Inconclusive {
                why: "asked for an object the model did not invent on these cases \
                      either, so there is no failure here for the prompt to explain",
            };
        }

        if self.abstention_prose_invented == 0 && self.abstention_prose_abstained > 0 {
            PromptEffect::ThePromptTakesTheDecision
        } else {
            PromptEffect::InventsHoweverItIsAsked
        }
    }
}

/// What the prose arm showed about the prompt's part in abstention failing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptEffect {
    /// Asked for an object it invented; asked in prose it declined instead, and
    /// it could still find the module where there was one.
    ///
    /// The failure belongs to how Thalyx asks, which is the one of the three
    /// suspects Thalyx can fix outright.
    ThePromptTakesTheDecision,
    /// It invented with the grammar, without it, and with nothing asking for an
    /// object at all.
    ///
    /// Neither the grammar nor the framing. What is left is the model, and the
    /// answer to that is a different family or a fine-tune — see
    /// `Debate-Agente-Fine-Tuning.md`, which rules the second out of Phase 1.
    InventsHoweverItIsAsked,
    /// The prose arm could not carry a verdict, and this says why.
    Inconclusive { why: &'static str },
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

    /// Two arms measured and the prose arm absent, which is the shape every
    /// test about the grammar verdict wants: the prose arm must not be able to
    /// move it.
    fn arms(constrained: Named, free: Named, wants_abstention: bool) -> ThreeArms {
        ThreeArms {
            constrained: Ok(constrained),
            unconstrained: Ok(free),
            prose: Err("not run".to_string()),
            wants_abstention,
            expected: Some("dev.thalyx.demo".to_string()),
        }
    }

    fn three(constrained: Named, free: Named, prose: Named, wants_abstention: bool) -> ThreeArms {
        ThreeArms {
            prose: Ok(prose),
            ..arms(constrained, free, wants_abstention)
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
    fn an_empty_target_list_is_a_decision_and_not_a_model_that_went_quiet() {
        // The defect this replaced, found in the 3B's first successful run of
        // this probe. Asked `instala algo bueno` under the grammar, it emitted
        // an empty target list — which is exactly the abstention six bench runs
        // had reported as zero out of forty-six — and the probe printed "said
        // something, named nothing", the same words it uses for a paragraph
        // that rambled. The one observation the whole investigation was looking
        // for was written down as noise.
        let transcript = told("instala algo bueno");
        assert_eq!(
            what_an_answer_named(
                r#"{"operation": "install_module", "targets": []} [end of text]"#,
                &transcript,
            ),
            Named::Abstained
        );
    }

    #[test]
    fn abstaining_is_not_the_same_word_as_generating_nothing_or_as_naming_nothing() {
        // Three things a human reading a run has to be able to tell apart, and
        // this module has now folded two of them together twice. Stated once,
        // as a claim, so a third folding fails here rather than in a run.
        let transcript = told("instala algo bueno");
        let abstained = what_an_answer_named(
            r#"{"operation": "install_module", "targets": []}"#,
            &transcript,
        );
        let generated_nothing = what_an_answer_named(" [end of text]", &transcript);
        let named_nothing = what_an_answer_named("I could not find one.", &transcript);

        assert_eq!(abstained, Named::Abstained);
        assert_eq!(generated_nothing, Named::SaidNothingAtAll);
        assert_eq!(named_nothing, Named::Nothing);
        assert_ne!(abstained, generated_nothing);
        assert_ne!(abstained, named_nothing);
    }

    #[test]
    fn the_constrained_column_is_counted_over_the_cases_the_constrained_arm_answered() {
        // The light tier's run printed "with the grammar, invented 6" directly
        // under "abstention cases measured on both arms 2", because the
        // constrained counts were reported against a denominator that drops
        // every case where the *free* arm generated nothing. Six out of two is
        // not a number anyone can act on.
        let mut tally = Tally::default();
        for _ in 0..6 {
            tally.count(&arms(
                Named::Invented("org.openjdk.jmh".to_string()),
                Named::SaidNothingAtAll,
                true,
            ));
        }
        for _ in 0..2 {
            tally.count(&arms(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                true,
            ));
        }

        assert_eq!(tally.abstention_paired, 8);
        assert_eq!(tally.abstention_constrained_invented, 6);
        assert_eq!(tally.abstention_measured(), 2);
        assert!(
            tally.abstention_constrained_invented <= tally.abstention_paired,
            "the constrained column is printed over a denominator smaller than itself"
        );
    }

    #[test]
    fn an_abstention_is_counted_on_whichever_arm_produced_it() {
        let mut tally = Tally::default();
        tally.count(&arms(Named::Abstained, Named::Abstained, true));
        tally.count(&arms(
            Named::Abstained,
            Named::Invented("com.adobe.photoshop".to_string()),
            true,
        ));

        assert_eq!(tally.abstention_constrained_abstained, 2);
        assert_eq!(tally.abstention_free_abstained, 1);
        assert_eq!(tally.abstention_free_invented, 1);
        assert_eq!(
            tally.abstention_measured(),
            2,
            "an empty proposal is an answer, so it belongs in what was compared"
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
    fn the_prose_arm_declines_only_with_the_word_it_was_given() {
        let transcript = told("instala algo bueno");

        assert_eq!(what_prose_named("NOTHING", &transcript), Named::Abstained);
        assert_eq!(
            what_prose_named("The answer is NOTHING.\n", &transcript),
            Named::Abstained
        );

        // The bias, asserted rather than left as a footnote. A model declining
        // in its own words is not counted as declining, and pretending
        // otherwise would be a prose reader built from fixtures its author
        // invented — the mistake this module exists downstream of.
        assert_eq!(
            what_prose_named("There is nothing here that names a module.", &transcript),
            Named::Nothing,
            "a lower-case `nothing` in ordinary prose was read as the token"
        );
    }

    #[test]
    fn naming_a_module_outranks_declining_however_the_sentence_is_hedged() {
        // "NOTHING matches exactly, but dev.thalyx.demo is close" is a proposal
        // with a hedge in front of it, not a refusal. Counting the hedge would
        // score a proposal as an abstention, which is the direction that makes
        // the prompt look innocent.
        let transcript = told("Thalyx's own records say: dev.thalyx.demo 1.4.2");
        assert_eq!(
            what_prose_named(
                "NOTHING matches exactly, but dev.thalyx.demo is close",
                &transcript
            ),
            Named::Attributable("dev.thalyx.demo".to_string())
        );
        assert_eq!(
            what_prose_named(
                "NOTHING here, unless you meant org.openjdk.jmh",
                &transcript
            ),
            Named::Invented("org.openjdk.jmh".to_string())
        );
    }

    #[test]
    fn a_prose_arm_that_generated_nothing_is_not_a_prose_arm_that_declined() {
        // The light tier answered twenty of twenty this way on the object
        // prompt, and it is the arm most likely to do it again here.
        let transcript = told("instala algo bueno");
        assert_eq!(
            what_prose_named(" [end of text]", &transcript),
            Named::SaidNothingAtAll
        );
    }

    #[test]
    fn declining_in_prose_where_the_object_arms_invented_blames_the_prompt() {
        let mut tally = Tally::default();
        for _ in 0..4 {
            tally.count(&three(
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Abstained,
                true,
            ));
        }
        for _ in 0..4 {
            tally.count(&three(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                false,
            ));
        }

        assert_eq!(tally.verdict(), Effect::InventsEitherWay);
        assert_eq!(
            tally.prompt_verdict(),
            PromptEffect::ThePromptTakesTheDecision
        );
    }

    #[test]
    fn inventing_in_prose_too_leaves_only_the_model() {
        let mut tally = Tally::default();
        for _ in 0..4 {
            tally.count(&three(
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Invented("com.adobe.photoshop".to_string()),
                true,
            ));
        }
        for _ in 0..4 {
            tally.count(&three(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                false,
            ));
        }

        assert_eq!(
            tally.prompt_verdict(),
            PromptEffect::InventsHoweverItIsAsked
        );
    }

    #[test]
    fn a_prose_arm_that_cannot_find_a_module_anywhere_carries_no_verdict_either() {
        // Rule 4 again, on the new arm and on its own denominator. Without this
        // a model that simply does not answer prose questions would look like
        // one whose declining proves the prompt guilty.
        let mut tally = Tally::default();
        for _ in 0..4 {
            tally.count(&three(
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Abstained,
                true,
            ));
        }
        for _ in 0..4 {
            tally.count(&three(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Abstained,
                false,
            ));
        }

        assert!(!tally.prose_control_holds());
        assert!(matches!(
            tally.prompt_verdict(),
            PromptEffect::Inconclusive { .. }
        ));
    }

    #[test]
    fn the_prose_arm_cannot_move_the_grammar_verdict_and_the_reverse() {
        // Two questions, two verdicts, counted separately because they fail
        // separately. A prose arm that ran out of tokens must not discard a
        // comparison between the two arms that did run.
        let mut with_prose = Tally::default();
        let mut without_prose = Tally::default();
        for _ in 0..4 {
            with_prose.count(&three(
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::SaidNothingAtAll,
                true,
            ));
            without_prose.count(&arms(
                Named::Invented("com.adobe.photoshop".to_string()),
                Named::Invented("com.adobe.photoshop".to_string()),
                true,
            ));
        }
        for _ in 0..4 {
            with_prose.count(&three(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::SaidNothingAtAll,
                false,
            ));
            without_prose.count(&arms(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                false,
            ));
        }

        assert_eq!(with_prose.verdict(), without_prose.verdict());
        assert_eq!(with_prose.verdict(), Effect::InventsEitherWay);
        assert!(matches!(
            with_prose.prompt_verdict(),
            PromptEffect::Inconclusive { .. }
        ));
    }

    #[test]
    fn a_prompt_has_nothing_to_answer_for_where_the_object_arms_did_not_fail() {
        let mut tally = Tally::default();
        for _ in 0..4 {
            tally.count(&three(
                Named::Abstained,
                Named::Abstained,
                Named::Abstained,
                true,
            ));
        }
        for _ in 0..4 {
            tally.count(&three(
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                Named::Attributable("dev.thalyx.demo".to_string()),
                false,
            ));
        }

        assert!(tally.prose_control_holds());
        assert!(matches!(
            tally.prompt_verdict(),
            PromptEffect::Inconclusive { .. }
        ));
    }

    #[test]
    fn a_case_whose_arm_failed_is_counted_in_nothing() {
        // The defect this forestalls is the one the bench already had once: a
        // run that never happened counted as a result. Here it would be worse,
        // because a missing free arm would read as silence, and silence is one
        // of the two answers.
        let mut tally = Tally::default();
        tally.count(&ThreeArms {
            constrained: Ok(Named::Invented("org.openjdk.jmh".to_string())),
            unconstrained: Err("the model ran out of tokens".to_string()),
            prose: Err("the model ran out of tokens".to_string()),
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
            Named::Abstained,
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
