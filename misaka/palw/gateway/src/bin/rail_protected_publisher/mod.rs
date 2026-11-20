//! Explicit local bridge from a Studio outbox job to its public material.
//! The publisher receives no signing key, funding input, or RPC endpoint.
use kaspa_hashes::Hash64;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    publisher: PathBuf,
    tokenizer: PathBuf,
    hf_descriptor: PathBuf,
    output_root: PathBuf,
    artifact: Option<PathBuf>,
    replay_budget: Option<PathBuf>,
    /// Complete private declaration used by the native quote, not an arbitrary
    /// ruleset ID. Omission retains the existing pinned publisher behavior.
    protected_planning: Option<PathBuf>,
}

pub(super) struct Published {
    owned_attempt: PathBuf,
    pub(super) binding: PathBuf,
    pub(super) material: PathBuf,
}

impl Published {
    /// Only the caller's pre-signing quote failure may discard this attempt.
    /// Do not remove any publication after signing or submission has begun.
    pub(super) fn discard_unquoted(self) -> Result<(), String> {
        std::fs::remove_dir_all(&self.owned_attempt)
            .map_err(|e| format!("cannot remove unquoted publication {}: {e}", self.owned_attempt.display()))
    }
}

impl Config {
    fn load(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        File::open(path).and_then(|f| f.take(16_385).read_to_end(&mut bytes)).map_err(|e| e.to_string())?;
        if bytes.len() > 16_384 {
            return Err("publisher config exceeds 16 KiB".into());
        }
        let config: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        for p in [&config.publisher, &config.tokenizer, &config.hf_descriptor, &config.output_root]
            .into_iter()
            .chain(config.artifact.iter())
            .chain(config.replay_budget.iter())
            .chain(config.protected_planning.iter())
        {
            if !p.is_absolute() {
                return Err("publisher config paths must be absolute".into());
            }
        }
        if config.artifact.is_some() != config.replay_budget.is_some() {
            return Err("publisher artifact and replay_budget must be configured together".into());
        }
        Ok(config)
    }
}

pub(super) fn publish(config: &Path, stem: &Path, capture: &Path, claim: Hash64) -> Result<Published, String> {
    let config = Config::load(config)?;
    let unsigned = PathBuf::from(format!("{}.commitment-unsigned.borsh", stem.display()));
    let request = PathBuf::from(format!("{}.request.borsh", stem.display()));
    let result = PathBuf::from(format!("{}.result.borsh", stem.display()));
    for path in [&unsigned, &request, &result, capture, &config.tokenizer, &config.hf_descriptor] {
        if !path.is_file() {
            return Err(format!("publisher input unavailable: {}", path.display()));
        }
    }
    if config.protected_planning.as_ref().is_some_and(|path| !path.is_file()) {
        return Err("publisher input unavailable: protected planning declaration".into());
    }
    std::fs::create_dir_all(&config.output_root).map_err(|e| e.to_string())?;
    // Each attempt gets a new destination. A failed or stale publication is never
    // silently reused, and retained bytes from earlier attempts are not replaced.
    let attempt = config.output_root.join(format!("{claim}-{:016x}", rand::random::<u64>()));
    std::fs::create_dir(&attempt).map_err(|e| e.to_string())?;
    struct AttemptDir {
        path: PathBuf,
        complete: bool,
    }
    impl Drop for AttemptDir {
        fn drop(&mut self) {
            if !self.complete {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }
    let mut owned = AttemptDir { path: attempt.clone(), complete: false };
    let directory = attempt.join("material");
    let mut child = Command::new(&config.publisher);
    child
        .args(["--commitment"])
        .arg(unsigned)
        .arg("--worker-request")
        .arg(request)
        .arg("--worker-result")
        .arg(result)
        .arg("--capture")
        .arg(capture)
        .arg("--tokenizer")
        .arg(&config.tokenizer)
        .arg("--hf-descriptor")
        .arg(&config.hf_descriptor)
        .arg("--out-directory")
        .arg(&directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let (Some(artifact), Some(budget)) = (&config.artifact, &config.replay_budget) {
        child.arg("--artifact").arg(artifact).arg("--replay-budget").arg(budget);
    }
    if let Some(planning) = &config.protected_planning {
        child.arg("--protected-planning").arg(planning);
    }
    let status = child.status().map_err(|e| format!("publisher could not start: {e}"))?;
    if !status.success() {
        return Err(format!("publisher did not finish successfully: {status}"));
    }
    let published = Published {
        owned_attempt: attempt,
        binding: directory.join("public-binding.hex"),
        material: directory.join("protected-material.hex"),
    };
    if !published.binding.is_file() || !published.material.is_file() {
        return Err("publisher did not finish both binding and material files".into());
    }
    // Success here only locates files. The rail decodes them, gets a native quote,
    // and authenticates that quote against this exact commitment before signing.
    owned.complete = true;
    Ok(published)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn protected_publisher_bridge_requires_complete_success_and_preserves_argument_boundaries() {
        let root = std::env::temp_dir().join(format!("misaka publisher bridge {:016x}", rand::random::<u64>()));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let stem = root.join("job with spaces");
        for suffix in ["commitment-unsigned.borsh", "request.borsh", "result.borsh"] {
            std::fs::write(format!("{}.{}", stem.display(), suffix), b"input").unwrap();
        }
        let capture = root.join("capture.bin");
        let tokenizer = root.join("tokenizer.json");
        let descriptor = root.join("descriptor.bin");
        for path in [&capture, &tokenizer, &descriptor] {
            std::fs::write(path, b"input").unwrap();
        }
        let publisher = root.join("publisher");
        let config = root.join("config.json");
        std::fs::write(
            &config,
            serde_json::to_vec(&serde_json::json!({
                "publisher": publisher, "tokenizer": tokenizer, "hf_descriptor": descriptor,
                "output_root": root.join("published"),
            }))
            .unwrap(),
        )
        .unwrap();
        let script = |text: &str| {
            std::fs::write(&publisher, text).unwrap();
            std::fs::set_permissions(&publisher, std::fs::Permissions::from_mode(0o700)).unwrap();
        };
        let claim = Hash64::from_u64_word(7);
        script("#!/bin/sh\nexit 9\n");
        assert!(publish(&config, &stem, &capture, claim).is_err());
        script("#!/bin/sh\nexit 0\n");
        assert!(publish(&config, &stem, &capture, claim).is_err());
        assert_eq!(std::fs::read_dir(root.join("published")).unwrap().count(), 0, "failed attempts leave no partial publication");
        script(
            r#"#!/bin/sh
set -eu
count=0
planning=''
while [ "$#" -gt 0 ]; do
 flag="$1"; value="$2"; shift 2
 case "$flag" in
  --commitment|--worker-request|--worker-result|--capture|--tokenizer|--hf-descriptor) test -f "$value"; count=$((count+1));;
  --out-directory) output="$value";;
  --protected-planning) test -f "$value"; planning="$value";;
  *) exit 10;;
 esac
done
test "$count" -eq 6
mkdir "$output"
printf 00 > "$output/public-binding.hex"
printf 00 > "$output/protected-material.hex"
printf '%s' "$planning" > "$output/planning-path"
"#,
        );
        let first = publish(&config, &stem, &capture, claim).unwrap();
        let second = publish(&config, &stem, &capture, claim).unwrap();
        assert_ne!(first.material, second.material);
        assert!(first.material.is_file() && first.binding.is_file());
        let discarded = first.material.clone();
        first.discard_unquoted().unwrap();
        assert!(!discarded.exists(), "quote failure removes only its unsigned attempt");
        assert!(second.material.is_file() && second.binding.is_file(), "other publications remain retained");
        assert_eq!(std::fs::read(second.material.parent().unwrap().join("planning-path")).unwrap(), b"");
        let planning = root.join("complete planning with spaces.json");
        std::fs::write(&planning, b"declaration passed as a file, validated by the publisher").unwrap();
        let mut configured: serde_json::Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        configured["protected_planning"] = serde_json::json!(planning);
        std::fs::write(&config, serde_json::to_vec(&configured).unwrap()).unwrap();
        let explicit = publish(&config, &stem, &capture, claim).unwrap();
        assert_eq!(
            std::fs::read_to_string(explicit.material.parent().unwrap().join("planning-path")).unwrap(),
            planning.to_str().unwrap(),
        );
        std::fs::remove_file(&planning).unwrap();
        assert!(publish(&config, &stem, &capture, claim).is_err(), "an absent explicit declaration cannot fall back");
        assert!(explicit.material.is_file() && second.material.is_file());
        configured["protected_planning"] = serde_json::json!("relative.json");
        std::fs::write(&config, serde_json::to_vec(&configured).unwrap()).unwrap();
        assert!(Config::load(&config).is_err());
        configured.as_object_mut().unwrap().remove("protected_planning");
        std::fs::write(&config, serde_json::to_vec(&configured).unwrap()).unwrap();
        std::fs::remove_file(format!("{}.request.borsh", stem.display())).unwrap();
        assert!(publish(&config, &stem, &capture, claim).is_err());
    }
}
