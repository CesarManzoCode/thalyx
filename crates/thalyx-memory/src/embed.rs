//! Turning text into vectors, and searching them.
//!
//! `vault/03-Primitivas/Memoria-Persistente.md` decrees a vector database, and
//! it was reaffirmed after a proposal to replace it with two SQLite tables
//! keyed by task id. The decree stands, so this is a vector store: written
//! here rather than taken from a crate, because the decree on the sandbox
//! applies to the same question — Thalyx implements its own.
//!
//! ## The honest part
//!
//! A vector store is only *semantic* if the vectors come from a model that
//! understands the text. Thalyx has no local model yet — which model to run is
//! an open decree — so the embedder that ships today is **lexical**: a hashed
//! bag of words. It finds text that shares words. It does not find text that
//! means the same thing in different words, and calling what it does "semantic
//! search" would be the same lie as an index that says it is current when it
//! is not.
//!
//! So every result carries whether the embedder that produced it was semantic,
//! the same way every graph query carries its freshness. A caller cannot get
//! matches without also being handed what kind of matching it was.
//!
//! Swapping in a real model changes one trait implementation and nothing else.
//!
//! ## Exact search, on purpose
//!
//! Similarity is computed against every stored vector. Approximate indexes
//! trade recall for speed, and a recall trade-off that has not been measured
//! is a memory that silently forgets. When the store outgrows a linear scan
//! that will be visible as a number, and the trade can be made deliberately.

/// A vector, always unit length.
///
/// Normalised at construction so similarity is a dot product and nothing
/// downstream has to remember to divide.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding {
    values: Vec<f32>,
}

impl Embedding {
    /// Build from raw values, normalising to unit length.
    ///
    /// An all-zero vector has no direction; it stays zero and is similar to
    /// nothing, which is the right answer for text with no usable words in it.
    pub fn new(mut values: Vec<f32>) -> Self {
        let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut values {
                *value /= norm;
            }
        }
        Self { values }
    }

    pub fn dimensions(&self) -> usize {
        self.values.len()
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Cosine similarity, which for unit vectors is the dot product.
    ///
    /// Returns 0 for mismatched dimensions rather than panicking or comparing
    /// a prefix: two vectors from different embedders are not more or less
    /// alike, they are incomparable.
    pub fn similarity(&self, other: &Embedding) -> f32 {
        if self.values.len() != other.values.len() {
            return 0.0;
        }
        self.values
            .iter()
            .zip(&other.values)
            .map(|(a, b)| a * b)
            .sum()
    }

    /// The exact bytes stored in the database.
    ///
    /// Little-endian `f32`, in order. Pinned by a test: a change of layout
    /// would not fail, it would silently make every stored vector mean
    /// something else and every recall return the wrong memories.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(4) {
            return None;
        }
        let values = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        // Already normalised when it was stored; rebuilding without
        // renormalising keeps a round trip exact.
        Some(Self { values })
    }
}

/// Something that turns text into a vector.
pub trait Embedder {
    fn dimensions(&self) -> usize;
    fn embed(&self, text: &str) -> Embedding;

    /// Whether this embedder understands meaning, or only words.
    ///
    /// Not decoration. It travels with every result so a caller cannot present
    /// lexical matches as semantic ones.
    fn is_semantic(&self) -> bool;

    /// A name for the stored vectors, so a store built with one embedder is
    /// not searched with another.
    fn name(&self) -> &str;
}

/// The dimension of the lexical embedder's vectors.
pub const LEXICAL_DIMENSIONS: usize = 256;

/// A hashed bag of words.
///
/// Every word is hashed into one of [`LEXICAL_DIMENSIONS`] buckets and counted.
/// Two texts are similar when they share words in similar proportions.
///
/// It is honest about being lexical, and it works today with no model, no
/// download and no network. That combination is worth more in Phase 1 than a
/// better embedder that cannot run yet.
pub struct LexicalEmbedder;

impl Embedder for LexicalEmbedder {
    fn dimensions(&self) -> usize {
        LEXICAL_DIMENSIONS
    }

    fn embed(&self, text: &str) -> Embedding {
        let mut buckets = vec![0.0f32; LEXICAL_DIMENSIONS];

        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
        {
            let lowered = word.to_lowercase();
            buckets[bucket_of(&lowered)] += 1.0;
        }

        Embedding::new(buckets)
    }

    fn is_semantic(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        "lexical-v1"
    }
}

/// FNV-1a, written out rather than taken from a crate.
///
/// The requirement is that the same word always lands in the same bucket, for
/// as long as a database lives. A hasher whose output could change between
/// releases would silently invalidate every stored vector, and the standard
/// library's does not promise stability.
fn bucket_of(word: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in word.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash % LEXICAL_DIMENSIONS as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_comes_out_unit_length() {
        let embedding = Embedding::new(vec![3.0, 4.0]);
        let norm: f32 = embedding.values().iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6, "norm was {norm}");
    }

    #[test]
    fn text_with_no_words_has_no_direction_and_matches_nothing() {
        let embedder = LexicalEmbedder;
        let empty = embedder.embed("   !!! ---   ");
        let something = embedder.embed("a real sentence");

        assert_eq!(empty.similarity(&something), 0.0);
        assert_eq!(empty.similarity(&empty), 0.0);
    }

    #[test]
    fn the_same_text_embeds_the_same_way_every_time() {
        // A database outlives the process that wrote it. An embedder whose
        // output drifted between runs would make every stored vector mean
        // something the next run cannot reproduce.
        let embedder = LexicalEmbedder;
        assert_eq!(
            embedder.embed("refactor the auth module"),
            embedder.embed("refactor the auth module")
        );
    }

    #[test]
    fn the_bucket_a_word_lands_in_is_fixed() {
        // Pinned deliberately. If this changes, every vector in every existing
        // database means something else, and nothing would report a problem.
        assert_eq!(bucket_of(""), 37);
        assert_eq!(bucket_of("auth"), 79);
        assert_eq!(bucket_of("refactor"), 179);
        assert_eq!(bucket_of("thalyx"), 253);
    }

    #[test]
    fn shared_words_score_higher_than_unrelated_ones() {
        let embedder = LexicalEmbedder;
        let subject = embedder.embed("refactor the auth module");
        let related = embedder.embed("the auth module needs a refactor");
        let unrelated = embedder.embed("compile the kernel with a new scheduler");

        assert!(
            subject.similarity(&related) > subject.similarity(&unrelated),
            "related {} vs unrelated {}",
            subject.similarity(&related),
            subject.similarity(&unrelated)
        );
    }

    #[test]
    fn case_and_punctuation_do_not_change_the_meaning() {
        let embedder = LexicalEmbedder;
        assert_eq!(
            embedder.embed("Refactor, the AUTH module!"),
            embedder.embed("refactor the auth module")
        );
    }

    #[test]
    fn the_lexical_embedder_admits_it_is_not_semantic() {
        // The property the whole honesty of `Recall` rests on. A caller must
        // never be able to present word overlap as understanding.
        assert!(!LexicalEmbedder.is_semantic());
    }

    #[test]
    fn different_words_for_the_same_idea_do_not_match_and_that_is_the_point() {
        // Recorded as a limitation rather than hidden: this is exactly what a
        // real model would fix, and exactly why the results say `semantic:
        // false` until one arrives.
        let embedder = LexicalEmbedder;
        let one = embedder.embed("the login code needs work");
        let other = embedder.embed("authentication requires attention");

        assert_eq!(
            one.similarity(&other),
            0.0,
            "a lexical embedder is not expected to connect these, and pretending \
             otherwise is the failure this flag exists to prevent"
        );
    }

    #[test]
    fn a_vector_round_trips_through_its_stored_bytes_exactly() {
        let embedder = LexicalEmbedder;
        let original = embedder.embed("install the module and grant it network");

        let bytes = original.to_bytes();
        assert_eq!(bytes.len(), LEXICAL_DIMENSIONS * 4);

        let restored = Embedding::from_bytes(&bytes).expect("valid bytes");
        assert_eq!(restored, original);
        assert!((restored.similarity(&original) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_stored_layout_is_little_endian_f32_in_order() {
        // Pinned: a change here would not fail, it would make every stored
        // vector mean something else and every recall return wrong memories.
        let embedding = Embedding {
            values: vec![1.0, -2.0],
        };
        assert_eq!(
            embedding.to_bytes(),
            [1.0f32.to_le_bytes(), (-2.0f32).to_le_bytes()].concat()
        );
    }

    #[test]
    fn bytes_that_are_not_a_whole_number_of_floats_are_refused() {
        assert!(Embedding::from_bytes(&[0, 1, 2]).is_none());
    }

    #[test]
    fn vectors_from_different_embedders_are_incomparable_not_merely_dissimilar() {
        let short = Embedding::new(vec![1.0, 0.0]);
        let long = Embedding::new(vec![1.0, 0.0, 0.0, 0.0]);
        assert_eq!(short.similarity(&long), 0.0);
    }
}
