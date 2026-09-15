use crate::{
    NetworkProxy,
    error::{Error, Result, fail},
};
use std::{collections::BTreeMap, process::Stdio, time::Duration};
use tokio::process::Command;

#[derive(Default)]
pub struct ResolvedProxy {
    pub http: String,
    pub https: String,
    pub bypass: String,
    pub bypass_local: bool,
}
impl ResolvedProxy {
    pub fn for_url(&self, url: &str) -> &str {
        if self.bypass_local
            && reqwest::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .is_some_and(|host| !host.contains('.') && !host.contains(':'))
        {
            return "";
        }
        if url.starts_with("https:") {
            &self.https
        } else {
            &self.http
        }
    }
}

async fn read(program: &str, args: &[&str]) -> Result<String> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Error::Message("无法读取系统代理，请使用自定义代理".into()))?;
    let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
        .await
        .map_err(|_| Error::Message("读取系统代理超时，请使用自定义代理".into()))??;
    if !output.status.success() {
        return fail("读取系统代理失败，请使用自定义代理");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn endpoint(host: &str, port: &str) -> Result<String> {
    let port: u16 = port
        .parse()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| Error::Message("系统代理端口无效，请检查系统设置".into()))?;
    let host = host.trim().trim_matches('\'');
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.into()
    };
    let url = format!("http://{host}:{port}");
    crate::network::validate_proxy(&NetworkProxy {
        mode: "manual".into(),
        url: url.clone(),
    })?;
    Ok(url)
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos(text: &str) -> Result<ResolvedProxy> {
    let mut entries = BTreeMap::new();
    let mut bypass = Vec::new();
    let mut exceptions = false;
    let mut nested = 0;
    for line in text.lines() {
        let line = line.trim();
        if nested > 0 {
            if line.ends_with('{') {
                nested += 1;
            }
            if line == "}" {
                nested -= 1;
            }
            continue;
        }
        if !exceptions
            && line.contains(" : ")
            && line.ends_with('{')
            && !line.starts_with("ExceptionsList :")
        {
            nested = 1;
            continue;
        }
        if line.starts_with("ExceptionsList :") {
            exceptions = true;
            continue;
        }
        if exceptions && line == "}" {
            exceptions = false;
            continue;
        }
        if let Some((key, value)) = line.split_once(" : ") {
            if exceptions {
                if value.contains('*') && !value.starts_with("*.") {
                    return fail("系统代理绕过规则暂不支持，请使用自定义代理");
                }
                bypass.push(value.trim_start_matches('*').to_string());
            } else {
                entries.insert(key, value);
            }
        }
    }
    let value = |key| entries.get(key).copied().unwrap_or("");
    if value("ProxyAutoConfigEnable") == "1" || value("ProxyAutoDiscoveryEnable") == "1" {
        return fail("系统启用了 PAC/自动发现代理，暂不支持解析，请选择自定义代理并填写固定地址");
    }
    if value("SOCKSEnable") == "1" && value("HTTPEnable") != "1" && value("HTTPSEnable") != "1" {
        return fail("系统仅启用了 SOCKS 代理，请使用 HTTP/HTTPS 自定义代理");
    }
    Ok(ResolvedProxy {
        http: if value("HTTPEnable") == "1" {
            endpoint(value("HTTPProxy"), value("HTTPPort"))?
        } else {
            String::new()
        },
        https: if value("HTTPSEnable") == "1" {
            endpoint(value("HTTPSProxy"), value("HTTPSPort"))?
        } else {
            String::new()
        },
        bypass: bypass.join(","),
        bypass_local: value("ExcludeSimpleHostnames") == "1",
    })
}

pub(crate) async fn resolve(proxy: &NetworkProxy) -> Result<ResolvedProxy> {
    crate::network::validate_proxy(proxy)?;
    match proxy.mode.as_str() {
        "manual" => {
            return Ok(ResolvedProxy {
                http: proxy.url.clone(),
                https: proxy.url.clone(),
                bypass: String::new(),
                bypass_local: false,
            });
        }
        "direct" => return Ok(ResolvedProxy::default()),
        _ => {}
    }
    #[cfg(target_os = "macos")]
    {
        return parse_macos(&read("/usr/sbin/scutil", &["--proxy"]).await?);
    }
    #[cfg(target_os = "windows")]
    {
        let output = read("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", "$p=Get-ItemProperty 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings'; $p | Select-Object ProxyEnable,ProxyServer,ProxyOverride,AutoConfigURL,AutoDetect | ConvertTo-Json -Compress"]).await?;
        let v: serde_json::Value = serde_json::from_str(&output)?;
        if v["AutoConfigURL"].as_str().is_some_and(|s| !s.is_empty())
            || v["AutoDetect"].as_u64() == Some(1)
        {
            return fail("系统使用 PAC/自动发现代理，请使用自定义代理");
        }
        if v["ProxyEnable"].as_u64() != Some(1) {
            return Ok(ResolvedProxy::default());
        }
        let server = v["ProxyServer"].as_str().unwrap_or("");
        let mut result = ResolvedProxy::default();
        for item in server.split(';') {
            let (kind, address) = item.split_once('=').unwrap_or(("all", item));
            if !["http", "https", "all"].contains(&kind) {
                continue;
            }
            let url = if address.contains("://") {
                address.to_string()
            } else {
                format!("http://{address}")
            };
            crate::network::validate_proxy(&NetworkProxy {
                mode: "manual".into(),
                url: url.clone(),
            })?;
            if kind != "https" {
                result.http = url.clone();
            }
            if kind != "http" {
                result.https = url;
            }
        }
        let bypass = v["ProxyOverride"].as_str().unwrap_or("");
        result.bypass_local = bypass.contains("<local>");
        result.bypass = bypass
            .split(';')
            .filter(|s| *s != "<local>")
            .collect::<Vec<_>>()
            .join(",")
            .replace("*.", ".");
        return Ok(result);
    }
    #[cfg(target_os = "linux")]
    {
        let mode = read("gsettings", &["get", "org.gnome.system.proxy", "mode"]).await?;
        if mode == "'none'" {
            return Ok(ResolvedProxy::default());
        }
        if mode != "'manual'" {
            return fail("系统使用自动代理或当前桌面不支持，请使用自定义代理");
        }
        let mut result = ResolvedProxy::default();
        for (schema, output) in [
            ("org.gnome.system.proxy.http", &mut result.http),
            ("org.gnome.system.proxy.https", &mut result.https),
        ] {
            let host = read("gsettings", &["get", schema, "host"]).await?;
            if host.trim_matches('\'').is_empty() {
                continue;
            }
            let port = read("gsettings", &["get", schema, "port"]).await?;
            *output = endpoint(&host, &port)?;
        }
        let bypass = read(
            "gsettings",
            &["get", "org.gnome.system.proxy", "ignore-hosts"],
        )
        .await?;
        result.bypass = bypass
            .trim_matches(['[', ']'])
            .replace('\'', "")
            .replace(' ', "")
            .replace("*.", ".");
        return Ok(result);
    }
    #[allow(unreachable_code)]
    fail("此系统暂不支持读取代理，请使用自定义代理")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macos_separates_protocols_and_preserves_exclusions() {
        let p = parse_macos("HTTPEnable : 0\nHTTPSEnable : 1\nHTTPSProxy : 127.0.0.1\nHTTPSPort : 7897\nExceptionsList : <array> {\n0 : *.local\n1 : 127.0.0.1\n}").unwrap();
        assert!(p.http.is_empty());
        assert_eq!(p.https, "http://127.0.0.1:7897");
        assert_eq!(p.bypass, ".local,127.0.0.1");
    }
    #[test]
    fn macos_ignores_scoped_overrides_and_bypasses_simple_hosts() {
        let p = parse_macos("HTTPSEnable : 1\nHTTPSProxy : 127.0.0.1\nHTTPSPort : 7897\nExcludeSimpleHostnames : 1\n__SCOPED__ : <dictionary> {\nHTTPSPort : 9999\n}").unwrap();
        assert_eq!(p.for_url("https://github.com"), "http://127.0.0.1:7897");
        assert_eq!(p.for_url("https://intranet"), "");
    }
    #[test]
    fn macos_rejects_pac_and_invalid_port_and_accepts_disabled() {
        assert!(parse_macos("ProxyAutoConfigEnable : 1").is_err());
        assert!(parse_macos("HTTPSEnable : 1\nHTTPSProxy : localhost\nHTTPSPort : 0").is_err());
        assert!(parse_macos("<dictionary> {\n}").unwrap().https.is_empty());
    }
}
