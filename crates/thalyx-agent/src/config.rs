//! Which tier is chosen, and which file the weights are.
//!
//! Thalyx never downloads a model. `vault/02-Arquitectura/Gamas-de-Modelo.md`
//! makes the tier a decision the user takes about their own hardware, and a
//! system that fetched several gigabytes on their behalf would be taking it for
//! them. So this records a choice and a path, and nothing here reaches the
//! network.
//!
//! ## Why the size is recorded and re-checked, and the digest is not
//!
//! The weights are outside the TCB — `vault/11-Seguridad/Modelo-de-Amenaza.md`
//! puts the model there, and the grammar plus attribution hold whatever the
//! model says. So a swapped file is not a security event, and hashing gigabytes
//! before every sentence somebody types would be paying a real cost for a
//! guarantee that is already held elsewhere.
//!
//! What a swapped file *is*, is a bench result attributed to the wrong weights.
//! So the digest is taken once, when the choice is recorded, and the size is
//! re-checked every time — cheap, and enough to notice that the file is not the
//! one that was measured. Noticing is a refusal rather than a warning, because
//! rule 9 says the cautious answer is the one a surprise gets.

use crate::llama::{Invocation, LlamaModel};
use crate::tier::Tier;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("`{0}` is not one of the tiers; `thalyx agent model` lists them")]
    NoSuchTier(String),

    #[error("the weights recorded for the {tier} tier are gone from {}", .path.display())]
    WeightsGone { tier: String, path: PathBuf },

    #[error(
        "the weights at {} are {found} bytes and {recorded} were recorded. \
         They are not the file the tier was set up against, so a measurement \
         taken now would be attributed to the wrong model. \
         Run `thalyx agent model use {tier} --weights <file>` to record what is \
         there now.",
        .path.display()
    )]
    WeightsChanged {
        tier: String,
        path: PathBuf,
        found: u64,
        recorded: u64,
    },

    #[error("the model settings at {} are unreadable: {source}", .path.display())]
    Unreadable {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// What was chosen, and what it was measured to be at the time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// One of `ligera`, `media`, `alta`, `maxima`.
    pub tier: String,
    pub weights: PathBuf,
    /// The llama.cpp binary. A bare name is looked up on `PATH`.
    pub binary: PathBuf,
    /// The flags that move between llama.cpp releases.
    ///
    /// Here rather than compiled in, so a build that rejects one is fixed by
    /// editing this file. See `llama.rs`.
    pub extra_args: Vec<String>,
    pub predict: u32,
    pub seed: u64,
    pub timeout_seconds: u64,
    /// The size of the weights when the tier was set up.
    ///
    /// **This is the measured figure the vault's table wants**, in place of the
    /// estimate in `tier.rs`. It is written here by the machine that saw the
    /// file rather than by anyone's recollection of a download page.
    pub weights_bytes: u64,
    /// `sha256:…`, taken once. See the module docs for why it is not re-checked.
    pub weights_digest: String,
    /// The installed module that runs the engine, when the engine is one.
    ///
    /// [`None`] means the engine is whatever `binary` names on `PATH`, which is
    /// what a development machine has and what `thalyx agent bench` runs
    /// against. On the machine itself there is no `PATH` and no libc: the
    /// engine is an installed, signed module, and this is its id.
    ///
    /// `#[serde(default)]` so that a settings file written before this field
    /// existed still reads — a machine that has to be reconfigured because
    /// Thalyx learned a new field is a machine that lost a human's decision to
    /// an upgrade.
    #[serde(default)]
    pub engine_module: Option<String>,
}

impl Settings {
    /// Record a choice, measuring the file rather than being told about it.
    pub fn record(
        tier: Tier,
        weights: impl Into<PathBuf>,
        binary: impl Into<PathBuf>,
    ) -> Result<Settings, ConfigError> {
        let weights = weights.into();
        Settings::record_reading(tier, weights.clone(), weights, binary)
    }

    /// The same, when the file is not yet where the machine will see it.
    ///
    /// `image/Makefile` is the one caller and the reason this exists: it builds
    /// the store on a development machine, where the weights sit in a staging
    /// directory, and records the path they will have **inside** Thalyx —
    /// `/opt/thalyx/data/engine/models/model.gguf`, which does not exist on the
    /// machine doing the building.
    ///
    /// The measurement still comes from the bytes. Recording a size and a
    /// digest that nobody read would be the opposite of what this file is for:
    /// see the module docs on why the size is re-checked at all.
    pub fn record_reading(
        tier: Tier,
        weights: impl Into<PathBuf>,
        reading: impl Into<PathBuf>,
        binary: impl Into<PathBuf>,
    ) -> Result<Settings, ConfigError> {
        let weights = weights.into();
        let reading = reading.into();
        let bytes = std::fs::metadata(&reading)?.len();
        let digest = digest_of(&reading)?;
        let defaults = Invocation::new(binary, &weights);

        Ok(Settings {
            tier: tier.name().to_string(),
            weights,
            binary: defaults.binary,
            extra_args: defaults.extra_args,
            predict: defaults.predict,
            seed: defaults.seed,
            timeout_seconds: defaults.timeout.as_secs(),
            weights_bytes: bytes,
            weights_digest: digest,
            engine_module: None,
        })
    }

    /// Record that the engine is an installed module rather than a `PATH` name.
    pub fn through_module(mut self, module_id: Option<String>) -> Settings {
        self.engine_module = module_id;
        self
    }

    pub fn tier(&self) -> Result<Tier, ConfigError> {
        Tier::parse(&self.tier).ok_or_else(|| ConfigError::NoSuchTier(self.tier.clone()))
    }

    pub fn load(path: &Path) -> Result<Option<Settings>, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        toml::from_str(&text)
            .map(Some)
            .map_err(|source| ConfigError::Unreadable {
                path: path.to_path_buf(),
                source,
            })
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).expect("Settings is plain data");
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Turn the settings into something that can be run, checking first.
    ///
    /// The check is here rather than in `llama.rs` because it is about the
    /// settings having gone stale, which is a thing only this file knows about.
    pub fn model(&self) -> Result<LlamaModel, ConfigError> {
        let found = match std::fs::metadata(&self.weights) {
            Ok(meta) => meta.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ConfigError::WeightsGone {
                    tier: self.tier.clone(),
                    path: self.weights.clone(),
                });
            }
            Err(error) => return Err(error.into()),
        };
        if found != self.weights_bytes {
            return Err(ConfigError::WeightsChanged {
                tier: self.tier.clone(),
                path: self.weights.clone(),
                found,
                recorded: self.weights_bytes,
            });
        }

        let mut invocation = Invocation::new(&self.binary, &self.weights);
        invocation.extra_args = self.extra_args.clone();
        invocation.predict = self.predict;
        invocation.seed = self.seed;
        invocation.timeout = Duration::from_secs(self.timeout_seconds);
        Ok(LlamaModel::new(invocation))
    }
}

/// `sha256:…` over a file, read in pieces.
///
/// In pieces because the smallest tier is about a gigabyte and the largest is
/// nine, and reading one of those into memory to hash it would make choosing a
/// tier fail on exactly the machines the tier exists for.
fn digest_of(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights(dir: &Path, contents: &[u8]) -> PathBuf {
        let path = dir.join("weights.gguf");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn a_choice_survives_being_written_and_read_back() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("state").join("agent-model.toml");
        let recorded =
            Settings::record(Tier::Medium, weights(scratch.path(), b"gguf"), "llama-cli").unwrap();

        recorded.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), Some(recorded));
    }

    #[test]
    fn nothing_configured_is_an_absence_rather_than_an_error() {
        // A machine with no model is a supported state, not a broken one — the
        // double route means everything the rules resolve still works. Reading
        // "no file" as a failure would make that state look like damage.
        let scratch = tempfile::tempdir().unwrap();
        assert_eq!(
            Settings::load(&scratch.path().join("agent-model.toml")).unwrap(),
            None
        );
    }

    #[test]
    fn the_size_written_down_is_the_size_of_the_file_and_not_the_estimate() {
        // This figure is what replaces the estimate in the vault's table, so it
        // has to come from the file rather than from the tier it was chosen as.
        let scratch = tempfile::tempdir().unwrap();
        let settings =
            Settings::record(Tier::Max, weights(scratch.path(), b"12345678"), "llama-cli").unwrap();

        assert_eq!(settings.weights_bytes, 8);
        assert_ne!(settings.weights_bytes, Tier::Max.disk().0);
    }

    #[test]
    fn weights_that_changed_since_they_were_recorded_are_refused_not_used() {
        // Rule 9. Running against a different file would produce a bench number
        // filed under the wrong model, and a wrong number that looks right is
        // worse than no number.
        let scratch = tempfile::tempdir().unwrap();
        let path = weights(scratch.path(), b"the model that was measured");
        let settings = Settings::record(Tier::Light, &path, "llama-cli").unwrap();

        std::fs::write(&path, b"a different model entirely, of another size").unwrap();

        let error = settings
            .model()
            .expect_err("the file is not the one measured");
        assert!(
            matches!(error, ConfigError::WeightsChanged { .. }),
            "got {error}"
        );
    }

    #[test]
    fn weights_that_are_gone_say_so_instead_of_saying_the_size_is_wrong() {
        // Rule 10: a failure to read is not a failure to exist, and the two send
        // whoever reads the message to different places.
        let scratch = tempfile::tempdir().unwrap();
        let path = weights(scratch.path(), b"gguf");
        let settings = Settings::record(Tier::Light, &path, "llama-cli").unwrap();
        std::fs::remove_file(&path).unwrap();

        assert!(matches!(
            settings.model().expect_err("the file is gone"),
            ConfigError::WeightsGone { .. }
        ));
    }

    #[test]
    fn the_digest_is_of_the_file_and_changes_with_it() {
        let scratch = tempfile::tempdir().unwrap();
        let one = Settings::record(Tier::High, weights(scratch.path(), b"aaaa"), "llama-cli")
            .unwrap()
            .weights_digest;
        let two = Settings::record(Tier::High, weights(scratch.path(), b"bbbb"), "llama-cli")
            .unwrap()
            .weights_digest;

        assert!(one.starts_with("sha256:"));
        assert_ne!(one, two);
    }

    #[test]
    fn settings_naming_a_tier_that_does_not_exist_are_refused_rather_than_guessed() {
        let scratch = tempfile::tempdir().unwrap();
        let mut settings =
            Settings::record(Tier::Light, weights(scratch.path(), b"gguf"), "llama-cli").unwrap();
        settings.tier = "enorme".to_string();

        assert!(matches!(
            settings.tier().expect_err("no such tier"),
            ConfigError::NoSuchTier(_)
        ));
    }

    #[test]
    fn the_flags_that_move_between_releases_are_in_the_file_and_can_be_edited() {
        // The whole reason they are settings: a llama.cpp that rejects one is
        // fixed by editing a line, not by rebuilding Thalyx.
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("agent-model.toml");
        let settings =
            Settings::record(Tier::Medium, weights(scratch.path(), b"gguf"), "llama-cli").unwrap();
        settings.save(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("extra_args"), "{text}");

        let edited = text.replace("\"-no-cnv\"", "\"--some-other-flag\"");
        std::fs::write(&path, edited).unwrap();
        let reloaded = Settings::load(&path).unwrap().unwrap();
        assert!(
            reloaded
                .extra_args
                .contains(&"--some-other-flag".to_string())
        );
    }
}
