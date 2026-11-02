//! GitHub update checker and build hash verification.

use std::path::Path;

pub const REPO_URL: &str = "https://github.com/RilleSB/MouseSetting502";
pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/RilleSB/MouseSetting502/releases/latest";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_GIT_HASH: &str = env!("BUILD_GIT_HASH");

#[derive(Debug, Clone)]
pub struct UpdateCheckResult {
    pub tag_name: String,
    pub release_url: String,
    pub asset_name: String,
    pub asset_sha256: Option<String>,
    pub local_sha256: String,
    pub is_newer: bool,
    pub hash_matches: bool,
}

pub fn open_repository() {
    let _ = webbrowser::open(REPO_URL);
}

pub fn open_releases(url: Option<&str>) {
    let target = url.unwrap_or("https://github.com/RilleSB/MouseSetting502/releases/latest");
    let _ = webbrowser::open(target);
}

/// Computes SHA-256 for a given file.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("Не удалось открыть файл: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Ошибка чтения файла: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize())
}

/// Returns the SHA-256 of the running executable.
pub fn current_executable_hash() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    sha256_file(&exe)
}

/// Queries the latest GitHub release via curl or PowerShell and verifies version & build hash.
pub fn check_for_updates() -> Result<UpdateCheckResult, String> {
    let local_sha256 = current_executable_hash().unwrap_or_else(|_| "недоступно".into());
    let stdout = fetch_releases_json()?;
    parse_release(&stdout, APP_VERSION, local_sha256)
}

fn parse_release(
    json: &[u8],
    current_version: &str,
    local_sha256: String,
) -> Result<UpdateCheckResult, String> {
    let val: serde_json::Value = serde_json::from_slice(json)
        .map_err(|e| format!("Некорректный ответ от GitHub API: {e}"))?;

    let tag_name = val["tag_name"].as_str().unwrap_or("").trim().to_string();
    if tag_name.is_empty() {
        return Err("GitHub не вернул версию последнего релиза".into());
    }

    let release_url = val["html_url"]
        .as_str()
        .unwrap_or("https://github.com/RilleSB/MouseSetting502/releases/latest")
        .to_string();

    let mut asset_name = String::new();
    let mut asset_sha256: Option<String> = None;

    if let Some(assets) = val["assets"].as_array() {
        // Keep the official Windows binary ahead of any additional executables.
        let preferred = assets
            .iter()
            .find(|asset| asset["name"].as_str() == Some("MouseSettings502.exe"));
        for asset in preferred.into_iter().chain(assets.iter()) {
            let name = asset["name"].as_str().unwrap_or("");
            if name.ends_with(".exe") {
                asset_name = name.to_string();
                if let Some(digest) = asset["digest"].as_str()
                    && let Some(hash) = digest.strip_prefix("sha256:")
                    && hash.len() == 64
                    && hash.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    asset_sha256 = Some(hash.to_ascii_lowercase());
                }
                break;
            }
        }
    }

    if asset_name.is_empty() {
        return Err(format!(
            "В релизе {tag_name} нет исполняемого файла для Windows"
        ));
    }

    let is_newer = compare_versions(&tag_name, current_version);
    let hash_matches = match (&asset_sha256, local_sha256.as_str()) {
        (Some(remote), local) if !local.is_empty() && local != "недоступно" => {
            remote.eq_ignore_ascii_case(local)
        }
        _ => false,
    };

    Ok(UpdateCheckResult {
        tag_name,
        release_url,
        asset_name,
        asset_sha256,
        local_sha256,
        is_newer,
        hash_matches,
    })
}

fn fetch_releases_json() -> Result<Vec<u8>, String> {
    // 1. Try curl.exe with CREATE_NO_WINDOW
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = std::process::Command::new("curl.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--connect-timeout",
        "10",
        "--max-time",
        "20",
        "-H",
        "User-Agent: MouseSetting502",
        "-H",
        "Accept: application/vnd.github+json",
        RELEASES_API_URL,
    ]);

    if let Ok(output) = cmd.output()
        && output.status.success()
        && !output.stdout.is_empty()
    {
        return Ok(output.stdout);
    }

    // 2. Fallback to PowerShell
    let mut ps = std::process::Command::new("powershell.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        ps.creation_flags(CREATE_NO_WINDOW);
    }
    ps.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!(
            "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $ErrorActionPreference = 'Stop'; (Invoke-RestMethod -Uri '{RELEASES_API_URL}' -TimeoutSec 20 -Headers @{{'User-Agent'='MouseSetting502';'Accept'='application/vnd.github+json'}}) | ConvertTo-Json -Compress -Depth 6"
        ),
    ]);

    let output = ps
        .output()
        .map_err(|e| format!("Не удалось выполнить сетевой запрос: {e}"))?;

    if !output.status.success() || output.stdout.is_empty() {
        return Err("Не удалось получить информацию о релизах с GitHub".into());
    }

    Ok(output.stdout)
}

/// Returns true if remote_tag is strictly newer than current_version.
fn compare_versions(remote_tag: &str, current_version: &str) -> bool {
    let clean = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|part| part.parse::<u32>().ok())
            .collect()
    };
    let remote = clean(remote_tag);
    let local = clean(current_version);
    remote > local
}

// ============================================================================
// Self-contained SHA-256 implementation (FIPS 180-4)
// ============================================================================

struct Sha256 {
    state: [u32; 8],
    count: u64,
    buffer: [u8; 64],
}

impl Sha256 {
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    fn new() -> Self {
        Self {
            state: Self::H0,
            count: 0,
            buffer: [0u8; 64],
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        let index = (self.count as usize) & 0x3f;
        self.count += data.len() as u64;

        if index > 0 {
            let left = 64 - index;
            if data.len() >= left {
                self.buffer[index..64].copy_from_slice(&data[..left]);
                self.transform(&self.buffer.clone());
                data = &data[left..];
            } else {
                self.buffer[index..index + data.len()].copy_from_slice(data);
                return;
            }
        }

        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.transform(&block);
            data = &data[64..];
        }

        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
        }
    }

    fn transform(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for (i, &word) in w.iter().enumerate() {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(Self::K[i])
                .wrapping_add(word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    fn finalize(mut self) -> String {
        let bit_len = self.count * 8;
        let mut pad = [0u8; 128];
        pad[0] = 0x80;
        let index = (self.count as usize) & 0x3f;
        let pad_len = if index < 56 { 56 - index } else { 120 - index };
        let total_pad = pad_len + 8;
        pad[pad_len..total_pad].copy_from_slice(&bit_len.to_be_bytes());
        self.update(&pad[..total_pad]);

        let mut res = String::with_capacity(64);
        for word in self.state {
            use std::fmt::Write;
            let _ = write!(res, "{word:08x}");
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty_string() {
        let mut hasher = Sha256::new();
        hasher.update(b"");
        assert_eq!(
            hasher.finalize(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_abc() {
        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        assert_eq!(
            hasher.finalize(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_version_comparison() {
        assert!(compare_versions("v1.0.1", "1.0.0"));
        assert!(compare_versions("v1.0.3", "1.0.2"));
        assert!(compare_versions("v2.0.0", "1.9.9"));
        assert!(!compare_versions("v1.0.0", "1.0.0"));
        assert!(!compare_versions("v0.9.5", "1.0.0"));
    }

    fn release_fixture(digest: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "tag_name": "v1.0.3",
            "html_url": "https://github.com/RilleSB/MouseSetting502/releases/tag/v1.0.3",
            "assets": [
                { "name": "helper.exe", "digest": null },
                { "name": "MouseSettings502.exe", "digest": digest }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn release_detects_update_and_selects_official_executable() {
        let hash = "ab".repeat(32);
        let json = release_fixture(serde_json::json!(format!("sha256:{hash}")));
        let result = parse_release(&json, "1.0.2", "cd".repeat(32)).unwrap();
        assert!(result.is_newer);
        assert!(!result.hash_matches);
        assert_eq!(result.asset_name, "MouseSettings502.exe");
        assert_eq!(result.asset_sha256, Some(hash));
    }

    #[test]
    fn installed_release_verifies_sha256_without_offering_update() {
        let hash = "ab".repeat(32);
        let json = release_fixture(serde_json::json!(format!("sha256:{}", hash.to_uppercase())));
        let result = parse_release(&json, "1.0.3", hash).unwrap();
        assert!(!result.is_newer);
        assert!(result.hash_matches);
    }

    #[test]
    fn missing_or_invalid_digest_never_verifies_local_file() {
        for digest in [
            serde_json::Value::Null,
            serde_json::json!("sha256:bad"),
            serde_json::json!(format!("sha512:{}", "ab".repeat(32))),
        ] {
            let result = parse_release(&release_fixture(digest), "1.0.3", "ab".repeat(32)).unwrap();
            assert!(result.asset_sha256.is_none());
            assert!(!result.hash_matches);
        }
    }

    #[test]
    fn incomplete_release_or_api_error_does_not_report_success() {
        for value in [
            serde_json::json!({ "message": "API rate limit exceeded" }),
            serde_json::json!({ "tag_name": "v1.0.3", "assets": [] }),
        ] {
            assert!(
                parse_release(&serde_json::to_vec(&value).unwrap(), "1.0.2", String::new())
                    .is_err()
            );
        }
    }
}
