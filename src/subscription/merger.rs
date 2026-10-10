use serde_yaml::{self, Mapping, Value};

/// Keys to preserve in the mihomo config during merge (infrastructure config).
const PRESERVE_KEYS: &[&str] = &[
    "mixed-port",
    "external-controller",
    "mode",
    "log-level",
    "allow-lan",
    "dns",
    "tun",
    "sniffer",
    "ipv6",
    "profile",
    "hosts",
    "interface-name",
    "routing-mark",
    "bind-address",
    "authentication",
    "tcp-concurrent",
    "geodata-mode",
    "geox-url",
    "unified-delay",
    "keep-alive-interval",
    "port",
    "socks-port",
    "redir-port",
    "tproxy-port",
    "find-process-mode",
];

/// Default template for new config.yaml when none exists.
const DEFAULT_TEMPLATE: &str = r#"mixed-port: 7897
external-controller: 127.0.0.1:9090
mode: rule
log-level: info
allow-lan: false
profile:
  store-selected: true
  store-fake-ip: true
dns:
  enable: true
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  fake-ip-filter:
    - '*.github.com'
    - github.com
  nameserver:
    - https://223.5.5.5/dns-query
    - https://doh.pub/dns-query
tun:
  enable: true
  stack: gvisor
  auto-route: true
  auto-detect-interface: true
  dns-hijack:
    - any:53
sniffer:
  enable: true
  sniffing:
    - tls
    - http
rules:
  - DST-PORT,22,DIRECT
"#;

pub struct MergerResult {
    pub yaml: String,
    pub proxy_count: usize,
    pub group_count: usize,
    pub rule_count: usize,
    pub warnings: Vec<String>,
}

/// Built-in mihomo policy targets that never need to exist in proxy-groups.
const BUILTIN_TARGETS: &[&str] = &["DIRECT", "REJECT", "REJECT-DROP", "PASS", "COMPATIBLE"];

/// Read a user extra-groups YAML file. Returns an empty list when the path is
/// empty or the file does not exist (feature disabled). Parse errors are hard
/// errors so the caller can surface them as warnings.
pub fn read_extra_groups(path: &str) -> Result<Vec<Value>, String> {
    if path.trim().is_empty() {
        return Ok(Vec::new());
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read extra-groups file '{}': {}", path, e)),
    };
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let val: Value = serde_yaml::from_str(&content)
        .map_err(|e| format!("extra-groups file '{}' YAML parse error: {}", path, e))?;
    match val {
        Value::Sequence(seq) => Ok(seq),
        _ => Err(format!(
            "extra-groups file '{}' must contain a YAML sequence of proxy-groups",
            path
        )),
    }
}

/// Extract the trailing policy target of a mihomo rule string.
///
/// Rules are comma-separated but logical rules (`AND,((...)),REJECT`) nest
/// commas inside parentheses, so the split must track bracket depth. Trailing
/// modifiers like `no-resolve` are skipped to find the real target.
fn rule_target(rule: &str) -> Option<&str> {
    let mut candidate = rule.trim();
    // Strip trailing modifiers, most common first.
    for modifier in ["no-resolve", "src"] {
        if let Some(rest) = candidate.strip_suffix(modifier) {
            candidate = rest.trim_end().strip_suffix(',').unwrap_or(candidate);
        }
    }
    let mut depth = 0i32;
    for (i, c) in candidate.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => depth -= 1,
            ',' if depth == 0 => return Some(candidate[i + 1..].trim()),
            _ => {}
        }
    }
    None
}

fn node_names(proxies: &Value) -> Vec<String> {
    proxies
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn group_names(groups: &Value) -> Vec<String> {
    groups
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .filter_map(|g| g.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn is_valid_target(target: &str, valid: &[String]) -> bool {
    BUILTIN_TARGETS.contains(&target) || valid.iter().any(|v| v == target)
}

/// Validate and filter user-defined extra groups against the subscription's
/// node and group inventory.
///
/// - Groups with a `filter` regex are kept only if it matches at least one
///   subscription node name.
/// - Groups with an explicit `proxies` list have unknown members pruned; a
///   group left with no members is dropped.
/// - Groups whose name collides with a subscription group are dropped
///   (subscription wins, because subscription rules may reference it).
/// - Cross-references between extra groups are honored in file order: write
///   referenced groups before the groups that reference them.
fn vet_extra_groups(
    extra_groups: &[Value],
    proxies: &Value,
    sub_groups: &Value,
) -> (Vec<Value>, Vec<String>) {
    let nodes = node_names(proxies);
    let sub_group_names = group_names(sub_groups);
    let mut warnings = Vec::new();
    let mut kept: Vec<Value> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for group in extra_groups {
        let name = group
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            warnings.push("extra group without a name was skipped".into());
            continue;
        }
        if sub_group_names.iter().any(|g| g == &name) {
            warnings.push(format!(
                "extra group '{}' skipped: name collides with a subscription group",
                name
            ));
            skipped.push(name);
            continue;
        }

        if let Some(filter) = group.get("filter").and_then(|f| f.as_str()) {
            let re = match regex::Regex::new(filter) {
                Ok(re) => re,
                Err(e) => {
                    warnings.push(format!(
                        "extra group '{}' skipped: invalid filter regex '{}': {}",
                        name, filter, e
                    ));
                    skipped.push(name);
                    continue;
                }
            };
            let matches = nodes.iter().filter(|n| re.is_match(n)).count();
            if matches == 0 {
                warnings.push(format!(
                    "extra group '{}' skipped: filter '{}' matched no subscription nodes",
                    name, filter
                ));
                skipped.push(name);
                continue;
            }
        }

        let mut group = group.clone();
        if let Some(members) = group.get("proxies").and_then(|p| p.as_sequence()) {
            // A group that references a skipped extra group is skipped whole:
            // keeping the remaining members would silently change its meaning.
            let depends_on_skipped = members.iter().any(|m| {
                m.as_str()
                    .map(|s| skipped.iter().any(|k| k == s))
                    .unwrap_or(false)
            });
            if depends_on_skipped {
                warnings.push(format!(
                    "extra group '{}' skipped: depends on a skipped extra group",
                    name
                ));
                skipped.push(name);
                continue;
            }

            let valid: Vec<String> = sub_group_names
                .iter()
                .cloned()
                .chain(
                    kept.iter()
                        .filter_map(|g| g.get("name").and_then(|n| n.as_str()).map(String::from)),
                )
                .chain(nodes.iter().cloned())
                .collect();
            let mut pruned: Vec<Value> = Vec::new();
            for member in members {
                let member_name = member.as_str().unwrap_or("");
                if is_valid_target(member_name, &valid) {
                    pruned.push(member.clone());
                } else {
                    warnings.push(format!(
                        "extra group '{}': member '{}' not found in subscription, pruned",
                        name, member_name
                    ));
                }
            }
            if pruned.is_empty() {
                warnings.push(format!(
                    "extra group '{}' skipped: all members were pruned",
                    name
                ));
                skipped.push(name);
                continue;
            }
            if let Some(map) = group.as_mapping_mut() {
                map.insert(Value::String("proxies".into()), Value::Sequence(pruned));
            }
        }

        kept.push(group);
    }

    (kept, warnings)
}

/// Validate extra_rules targets: a rule whose policy target does not exist
/// (not builtin, not a subscription node/group, not a surviving extra group)
/// is dropped with a warning instead of poisoning the whole config.
fn vet_extra_rules(extra_rules: &[String], valid_targets: &[String]) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut kept = Vec::new();
    for rule in extra_rules {
        match rule_target(rule) {
            Some(target) if is_valid_target(target, valid_targets) => kept.push(rule.clone()),
            Some(target) => warnings.push(format!(
                "extra rule '{}' dropped: target '{}' does not exist in subscription or extra groups",
                rule, target
            )),
            None => warnings.push(format!(
                "extra rule '{}' dropped: could not parse a policy target",
                rule
            )),
        }
    }
    (kept, warnings)
}

/// Merge subscription content into a mihomo config.yaml.
///
/// Reads the existing config (or uses default template), preserves infrastructure
/// keys, replaces proxies/proxy-groups/rules with subscription content, removes
/// proxy-providers. `extra_rules` are prepended ahead of the subscription rules
/// so they take precedence during top-down rule matching. `extra_groups`
/// (user-defined proxy-groups) are validated and appended after subscription
/// groups.
///
/// `sub_dns` is the subscription's `dns:` section, if any: its
/// `nameserver-policy` entries are merged into the config's dns (subscription
/// wins per-domain) so airport-specific resolvers steer node domains to the
/// correct edge IPs. Other dns keys stay under the user's config control.
pub fn merge_mihomo_config(
    config_path: &str,
    proxies: &Value,
    proxy_groups: &Value,
    rules: &Value,
    extra_rules: &[String],
    sub_dns: Option<&Value>,
    extra_groups: &[Value],
) -> Result<MergerResult, String> {
    let existing_yaml = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut config: Mapping = if existing_yaml.trim().is_empty() {
        serde_yaml::from_str(DEFAULT_TEMPLATE).map_err(|e| format!("template error: {}", e))?
    } else {
        let val: Value = serde_yaml::from_str(&existing_yaml)
            .map_err(|e| format!("config YAML parse error: {}", e))?;
        val.as_mapping().cloned().unwrap_or_default()
    };

    config.remove("proxy-providers");
    merge_nameserver_policy(&mut config, sub_dns);

    let (kept_groups, group_warnings) = vet_extra_groups(extra_groups, proxies, proxy_groups);
    let mut all_group_names = group_names(proxy_groups);
    for g in &kept_groups {
        if let Some(n) = g.get("name").and_then(|n| n.as_str()) {
            all_group_names.push(n.to_string());
        }
    }
    let mut valid_targets = all_group_names;
    valid_targets.extend(node_names(proxies));
    let (kept_rules, rule_warnings) = vet_extra_rules(extra_rules, &valid_targets);

    let merged_groups = {
        let mut seq = proxy_groups.as_sequence().cloned().unwrap_or_default();
        seq.extend(kept_groups);
        Value::Sequence(seq)
    };

    config.insert(Value::String("proxies".into()), proxies.clone());
    config.insert(Value::String("proxy-groups".into()), merged_groups.clone());
    let merged_rules = inject_extra_rules(rules, &kept_rules);
    config.insert(Value::String("rules".into()), merged_rules.clone());

    let mut ordered = Mapping::new();
    for &key in PRESERVE_KEYS {
        if let Some(v) = config.remove(key) {
            ordered.insert(Value::String(key.into()), v);
        }
    }
    let mut managed: Vec<(Value, Value)> = Vec::new();
    for key in &["proxies", "proxy-groups", "rules"] {
        if let Some(v) = config.remove(*key) {
            managed.push((Value::String(key.to_string()), v));
        }
    }
    let rest: Vec<(Value, Value)> = config.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (k, v) in rest {
        config.remove(&k);
        ordered.insert(k, v);
    }
    for (k, v) in managed {
        ordered.insert(k, v);
    }

    let yaml = serde_yaml::to_string(&Value::Mapping(ordered))
        .map_err(|e| format!("serialization error: {}", e))?;

    let proxy_count = count_sequence(proxies);
    let group_count = count_sequence(&merged_groups);
    let rule_count = count_sequence(&merged_rules);

    let mut warnings = group_warnings;
    warnings.extend(rule_warnings);

    Ok(MergerResult {
        yaml,
        proxy_count,
        group_count,
        rule_count,
        warnings,
    })
}

/// Merge the subscription's `dns.nameserver-policy` into the config's dns
/// section. Airport subscriptions use this to point their node domains at
/// private DoH resolvers that return reachable edge IPs; without it public
/// resolvers may hand out blocked CDN edges.
///
/// Only `nameserver-policy` is taken, per-domain, with subscription entries
/// overriding the same domain in the user's config. All other dns keys
/// (nameserver, fake-ip-filter, ...) are left untouched.
fn merge_nameserver_policy(config: &mut Mapping, sub_dns: Option<&Value>) {
    let Some(policy) = sub_dns
        .and_then(|dns| dns.get("nameserver-policy"))
        .and_then(|p| p.as_mapping())
        .filter(|p| !p.is_empty())
    else {
        return;
    };

    let dns_key = Value::String("dns".into());
    if !config.get(&dns_key).is_some_and(|v| v.is_mapping()) {
        config.insert(dns_key.clone(), Value::Mapping(Mapping::new()));
    }
    let Some(dns_map) = config.get_mut(&dns_key).and_then(|v| v.as_mapping_mut()) else {
        return;
    };

    let policy_key = Value::String("nameserver-policy".into());
    if !dns_map.get(&policy_key).is_some_and(|v| v.is_mapping()) {
        dns_map.insert(policy_key.clone(), Value::Mapping(Mapping::new()));
    }
    let Some(target) = dns_map
        .get_mut(&policy_key)
        .and_then(|v| v.as_mapping_mut())
    else {
        return;
    };

    for (domain, servers) in policy {
        target.insert(domain.clone(), servers.clone());
    }
}

fn inject_extra_rules(rules: &Value, extra: &[String]) -> Value {
    let Some(seq) = rules.as_sequence() else {
        return rules.clone();
    };
    if extra.is_empty() {
        return rules.clone();
    }
    let mut out: Vec<Value> = Vec::with_capacity(seq.len() + extra.len());
    for r in extra {
        out.push(Value::String(r.clone()));
    }
    out.extend(seq.iter().cloned());
    Value::Sequence(out)
}

fn count_sequence(val: &Value) -> usize {
    val.as_sequence().map(|s| s.len()).unwrap_or(0)
}

pub fn backup_file(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Ok(());
    }
    std::fs::copy(path, format!("{}.bak", path)).map_err(|e| format!("backup failed: {}", e))?;
    Ok(())
}

pub fn rollback_file(path: &str) -> Result<(), String> {
    let bak = format!("{}.bak", path);
    let p = std::path::Path::new(&bak);
    if !p.exists() {
        return Ok(());
    }
    std::fs::copy(&bak, path).map_err(|e| format!("rollback failed: {}", e))?;
    Ok(())
}

pub fn discard_backup(path: &str) {
    let _ = std::fs::remove_file(format!("{}.bak", path));
}

pub fn write_config(path: &str, yaml: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = format!("{}.tmp", path);
    std::fs::write(&tmp, yaml).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_merge_into_existing_config() {
        let existing = r#"mixed-port: 7897
external-controller: 127.0.0.1:9090
mode: rule
dns:
  enable: true
  nameserver: [8.8.8.8]
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, existing).unwrap();

        let full: Value = serde_yaml::from_str("proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G").unwrap();
        let proxies = full.get("proxies").unwrap();
        let proxy_groups = full.get("proxy-groups").unwrap();
        let rules = full.get("rules").unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            proxies,
            proxy_groups,
            rules,
            &[],
            None,
            &[],
        )
        .unwrap();

        assert!(result.yaml.contains("mixed-port: 7897"));
        assert!(result.yaml.contains("dns:"));
        assert!(result.yaml.contains("name: N1"));
        assert!(result.yaml.contains("name: G"));
        assert!(result.yaml.contains("MATCH,G"));
        assert!(!result.yaml.contains("proxy-providers"));
        assert_eq!(result.proxy_count, 1);
        assert_eq!(result.group_count, 1);
        assert_eq!(result.rule_count, 1);
    }

    #[test]
    fn test_merge_preserves_unknown_top_level_keys() {
        let existing = r#"mixed-port: 7897
secret: "abc"
external-ui: ./ui
rule-providers:
  rp:
    type: http
    url: https://example.com/rp.yaml
my-custom-key: 42
proxy-providers:
  pp:
    type: http
    url: https://example.com/pp.yaml
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, existing).unwrap();

        let full: Value = serde_yaml::from_str(
            "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G",
        )
        .unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            full.get("proxies").unwrap(),
            full.get("proxy-groups").unwrap(),
            full.get("rules").unwrap(),
            &[],
            None,
            &[],
        )
        .unwrap();

        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        assert_eq!(out.get("secret").and_then(|v| v.as_str()), Some("abc"));
        assert_eq!(out.get("my-custom-key").and_then(|v| v.as_i64()), Some(42));
        assert!(out.get("rule-providers").is_some());
        assert!(out.get("external-ui").is_some());
        assert!(out.get("proxy-providers").is_none());
        assert_eq!(out.get("proxies").unwrap().as_sequence().unwrap().len(), 1);
    }

    #[test]
    fn test_merge_orders_managed_sections_last() {
        let existing = r#"mixed-port: 7897
rules:
  - MATCH,DIRECT
my-custom-key: 42
proxies:
  - name: OLD
    type: ss
    server: 1.1.1.1
    port: 443
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, existing).unwrap();

        let full: Value = serde_yaml::from_str(
            "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G",
        )
        .unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            full.get("proxies").unwrap(),
            full.get("proxy-groups").unwrap(),
            full.get("rules").unwrap(),
            &[],
            None,
            &[],
        )
        .unwrap();

        let out: Mapping = serde_yaml::from_str(&result.yaml).unwrap();
        let keys: Vec<&str> = out.keys().filter_map(|k| k.as_str()).collect();
        assert_eq!(keys.first(), Some(&"mixed-port"));
        assert!(keys.ends_with(&["proxies", "proxy-groups", "rules"]));
        let custom = keys.iter().position(|k| *k == "my-custom-key").unwrap();
        let proxies_pos = keys.iter().position(|k| *k == "proxies").unwrap();
        assert!(custom < proxies_pos);
        assert!(!result.yaml.contains("OLD"));
    }

    #[test]
    fn test_extra_rules_prepended() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules: Value = serde_yaml::from_str("- DOMAIN-SUFFIX,x.com,G\n- MATCH,G").unwrap();
        let extra = vec!["AND,((NETWORK,udp),(DST-PORT,443)),REJECT".to_string()];

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &extra,
            None,
            &[],
        )
        .unwrap();

        assert_eq!(result.rule_count, 3);
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let seq = out.get("rules").unwrap().as_sequence().unwrap();
        let list: Vec<&str> = seq.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(
            list,
            vec![
                "AND,((NETWORK,udp),(DST-PORT,443)),REJECT",
                "DOMAIN-SUFFIX,x.com,G",
                "MATCH,G"
            ]
        );
    }

    #[test]
    fn test_extra_rules_prepended_without_match() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules: Value = serde_yaml::from_str("- DOMAIN-SUFFIX,x.com,G").unwrap();
        let extra = vec!["AND,((NETWORK,udp),(DST-PORT,443)),REJECT".to_string()];

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &extra,
            None,
            &[],
        )
        .unwrap();

        assert_eq!(result.rule_count, 2);
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let seq = out.get("rules").unwrap().as_sequence().unwrap();
        let list: Vec<&str> = seq.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(
            list,
            vec![
                "AND,((NETWORK,udp),(DST-PORT,443)),REJECT",
                "DOMAIN-SUFFIX,x.com,G"
            ]
        );
    }

    #[test]
    fn test_empty_extra_rules_keep_rules_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules: Value = serde_yaml::from_str("- DOMAIN-SUFFIX,x.com,G\n- MATCH,G").unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &[],
            None,
            &[],
        )
        .unwrap();

        assert_eq!(result.rule_count, 2);
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        assert_eq!(out.get("rules").unwrap().as_sequence().unwrap().len(), 2);
    }

    #[test]
    fn test_merge_with_default_template_when_no_config() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.yaml");

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules = Value::Sequence(vec![]);

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &[],
            None,
            &[],
        )
        .unwrap();

        assert!(result.yaml.contains("mixed-port: 7897"));
        assert!(result.yaml.contains("gvisor"));
        assert!(result.yaml.contains("fake-ip"));
        assert!(result.yaml.contains("store-fake-ip: true"));
    }

    #[test]
    fn test_backup_and_rollback() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "original content").unwrap();

        backup_file(path.to_str().unwrap()).unwrap();
        assert!(dir.path().join("config.yaml.bak").exists());

        std::fs::write(&path, "modified").unwrap();

        rollback_file(path.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original content");
    }

    #[test]
    fn test_write_config_creates_parent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir").join("config.yaml");
        write_config(path.to_str().unwrap(), "test content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "test content");
    }

    #[test]
    fn test_write_config_atomic_no_tmp_left() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        write_config(path.to_str().unwrap(), "content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");
        assert!(!dir.path().join("config.yaml.tmp").exists());
    }

    #[test]
    fn test_write_config_replaces_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "old").unwrap();
        write_config(path.to_str().unwrap(), "new content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
        assert!(!dir.path().join("config.yaml.tmp").exists());
    }

    #[test]
    fn test_discard_backup_removes_bak_silently() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "content").unwrap();
        std::fs::write(dir.path().join("config.yaml.bak"), "old").unwrap();

        discard_backup(path.to_str().unwrap());

        assert!(!dir.path().join("config.yaml.bak").exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");
        discard_backup(path.to_str().unwrap());
        assert!(!dir.path().join("config.yaml.bak").exists());
    }

    #[test]
    fn test_subscription_nameserver_policy_merged() {
        let existing = r#"mixed-port: 7897
dns:
  enable: true
  nameserver: [8.8.8.8]
  nameserver-policy:
    +.user.example: [https://user-doh.example/dns-query]
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, existing).unwrap();

        let full: Value = serde_yaml::from_str(
            "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G",
        )
        .unwrap();
        let sub_dns: Value = serde_yaml::from_str(
            "enable: true\nnameserver: [9.9.9.9]\nnameserver-policy:\n  +.quandao.com:\n    - https://api-d.dohcore.com:2096/dns-query/token\n  +.jiandaoyun.com:\n    - https://api-d.dohcore.com:2096/dns-query/token\n",
        )
        .unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            full.get("proxies").unwrap(),
            full.get("proxy-groups").unwrap(),
            full.get("rules").unwrap(),
            &[],
            Some(&sub_dns),
            &[],
        )
        .unwrap();

        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let dns = out.get("dns").unwrap();
        let policy = dns.get("nameserver-policy").unwrap().as_mapping().unwrap();
        assert!(policy.contains_key("+.quandao.com"));
        assert!(policy.contains_key("+.jiandaoyun.com"));
        assert!(
            policy.contains_key("+.user.example"),
            "user policy entries must be preserved"
        );
        // 订阅的其他 dns 键不得覆盖用户配置
        let nameserver = dns.get("nameserver").unwrap().as_sequence().unwrap();
        assert_eq!(nameserver.len(), 1);
        assert_eq!(nameserver[0].as_str(), Some("8.8.8.8"));
    }

    #[test]
    fn test_subscription_policy_overrides_same_domain() {
        let existing = r#"mixed-port: 7897
dns:
  nameserver-policy:
    +.quandao.com: [https://old.example/dns-query]
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, existing).unwrap();

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules = Value::Sequence(vec![]);
        let sub_dns: Value = serde_yaml::from_str(
            "nameserver-policy:\n  +.quandao.com: [https://new.example/dns-query]\n",
        )
        .unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &[],
            Some(&sub_dns),
            &[],
        )
        .unwrap();

        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let policy = out
            .get("dns")
            .unwrap()
            .get("nameserver-policy")
            .unwrap()
            .as_mapping()
            .unwrap();
        assert_eq!(
            policy.get("+.quandao.com").unwrap().as_sequence().unwrap()[0].as_str(),
            Some("https://new.example/dns-query")
        );
    }

    #[test]
    fn test_no_subscription_dns_keeps_policy_untouched() {
        let existing = r#"mixed-port: 7897
dns:
  nameserver-policy:
    +.user.example: [https://user.example/dns-query]
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, existing).unwrap();

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules = Value::Sequence(vec![]);

        // Case 1: no dns at all in subscription.
        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &[],
            None,
            &[],
        )
        .unwrap();
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let policy = out
            .get("dns")
            .unwrap()
            .get("nameserver-policy")
            .unwrap()
            .as_mapping()
            .unwrap();
        assert_eq!(policy.len(), 1);
        assert!(policy.contains_key("+.user.example"));

        // Case 2: subscription dns without nameserver-policy.
        let sub_dns: Value = serde_yaml::from_str("enable: true\nnameserver: [1.1.1.1]\n").unwrap();
        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &[],
            Some(&sub_dns),
            &[],
        )
        .unwrap();
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let policy = out
            .get("dns")
            .unwrap()
            .get("nameserver-policy")
            .unwrap()
            .as_mapping()
            .unwrap();
        assert_eq!(policy.len(), 1);
        assert!(policy.contains_key("+.user.example"));
    }

    #[test]
    fn test_policy_creates_dns_section_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules = Value::Sequence(vec![]);
        let sub_dns: Value = serde_yaml::from_str(
            "nameserver-policy:\n  +.quandao.com: [https://api-d.dohcore.com:2096/dns-query/token]\n",
        )
        .unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &proxy_groups,
            &rules,
            &[],
            Some(&sub_dns),
            &[],
        )
        .unwrap();

        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        assert!(out
            .get("dns")
            .unwrap()
            .get("nameserver-policy")
            .unwrap()
            .as_mapping()
            .unwrap()
            .contains_key("+.quandao.com"));
    }

    fn sub_nodes() -> Value {
        serde_yaml::from_str(
            "- name: '🇭🇰1香港-专线(AnyTLS)'\n  type: ss\n  server: 1.1.1.1\n  port: 443\n- name: '🇭🇰2香港-专线(AnyTLS)'\n  type: ss\n  server: 1.1.1.2\n  port: 443\n- name: '4台湾-专线(AnyTLS)'\n  type: ss\n  server: 2.2.2.2\n  port: 443\n- name: '🇺🇸12美国旧金山-专线(AnyTLS)'\n  type: ss\n  server: 3.3.3.3\n  port: 443\n",
        )
        .unwrap()
    }

    fn sub_groups_fixture() -> Value {
        serde_yaml::from_str("- name: G\n  type: select\n  proxies: ['🇭🇰1香港-专线(AnyTLS)']\n")
            .unwrap()
    }

    fn argo_extra_groups() -> Vec<Value> {
        let pool: Value = serde_yaml::from_str(
            "name: argo-asia-auto\ntype: fallback\ninclude-all: true\nfilter: \"(香港|台湾|日本|新加坡).*(专线)\"\nurl: https://www.cloudflare.com/cdn-cgi/trace\ninterval: 120\n",
        )
        .unwrap();
        let shell: Value = serde_yaml::from_str(
            "name: argo-hk\ntype: fallback\nproxies: [argo-asia-auto, DIRECT]\nurl: https://www.cloudflare.com/cdn-cgi/trace\ninterval: 120\n",
        )
        .unwrap();
        vec![pool, shell]
    }

    #[test]
    fn test_extra_groups_appended_after_subscription_groups() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let proxies = sub_nodes();
        let groups = sub_groups_fixture();
        let rules: Value = serde_yaml::from_str("- MATCH,G").unwrap();
        let extra_rules = vec!["DOMAIN-SUFFIX,argotunnel.com,argo-hk".to_string()];

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies,
            &groups,
            &rules,
            &extra_rules,
            None,
            &argo_extra_groups(),
        )
        .unwrap();

        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.group_count, 3);
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let group_names: Vec<&str> = out
            .get("proxy-groups")
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|g| g.get("name").and_then(|n| n.as_str()))
            .collect();
        assert_eq!(group_names, vec!["G", "argo-asia-auto", "argo-hk"]);
        let rule_list: Vec<&str> = out
            .get("rules")
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|r| r.as_str())
            .collect();
        assert_eq!(
            rule_list,
            vec!["DOMAIN-SUFFIX,argotunnel.com,argo-hk", "MATCH,G"]
        );
    }

    #[test]
    fn test_extra_group_filter_no_match_skipped_and_rule_dropped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let pool: Value = serde_yaml::from_str(
            "name: argo-asia-auto\ntype: fallback\ninclude-all: true\nfilter: \"(不存在的地区).*(专线)\"\n",
        )
        .unwrap();
        let shell: Value = serde_yaml::from_str(
            "name: argo-hk\ntype: fallback\nproxies: [argo-asia-auto, DIRECT]\n",
        )
        .unwrap();
        let extra_rules = vec!["DOMAIN-SUFFIX,argotunnel.com,argo-hk".to_string()];

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &sub_nodes(),
            &sub_groups_fixture(),
            &serde_yaml::from_str("- MATCH,G").unwrap(),
            &extra_rules,
            None,
            &[pool, shell],
        )
        .unwrap();

        assert_eq!(result.warnings.len(), 3, "{:?}", result.warnings);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("argo-asia-auto") && w.contains("matched no")));
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("argo-hk") && w.contains("depends on a skipped")));
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("argo-hk") && w.contains("does not exist")));
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let group_names: Vec<&str> = out
            .get("proxy-groups")
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|g| g.get("name").and_then(|n| n.as_str()))
            .collect();
        assert_eq!(group_names, vec!["G"]);
        let rule_list: Vec<&str> = out
            .get("rules")
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|r| r.as_str())
            .collect();
        assert_eq!(rule_list, vec!["MATCH,G"]);
    }

    #[test]
    fn test_extra_group_filter_partial_match_kept() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let pool: Value = serde_yaml::from_str(
            "name: argo-hk-tpe\ntype: fallback\ninclude-all: true\nfilter: \"(香港|台湾).*专线\"\n",
        )
        .unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &sub_nodes(),
            &sub_groups_fixture(),
            &serde_yaml::from_str("- MATCH,G").unwrap(),
            &[],
            None,
            &[pool],
        )
        .unwrap();

        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let group_names: Vec<&str> = out
            .get("proxy-groups")
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|g| g.get("name").and_then(|n| n.as_str()))
            .collect();
        assert_eq!(group_names, vec!["G", "argo-hk-tpe"]);
    }

    #[test]
    fn test_extra_group_explicit_members_pruned() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let group: Value = serde_yaml::from_str(
            "name: my-fallback\ntype: fallback\nproxies: ['🇭🇰1香港-专线(AnyTLS)', '不存在节点', DIRECT]\n",
        )
        .unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &sub_nodes(),
            &sub_groups_fixture(),
            &serde_yaml::from_str("- MATCH,G").unwrap(),
            &[],
            None,
            &[group],
        )
        .unwrap();

        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("不存在节点") && w.contains("pruned")));
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let groups = out.get("proxy-groups").unwrap().as_sequence().unwrap();
        let my = groups
            .iter()
            .find(|g| g.get("name").and_then(|n| n.as_str()) == Some("my-fallback"))
            .unwrap();
        let members: Vec<&str> = my
            .get("proxies")
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|m| m.as_str())
            .collect();
        assert_eq!(members, vec!["🇭🇰1香港-专线(AnyTLS)", "DIRECT"]);
    }

    #[test]
    fn test_extra_group_name_collision_with_sub_group_skipped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let group: Value =
            serde_yaml::from_str("name: G\ntype: fallback\nproxies: [DIRECT]\n").unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &sub_nodes(),
            &sub_groups_fixture(),
            &serde_yaml::from_str("- MATCH,G").unwrap(),
            &[],
            None,
            &[group],
        )
        .unwrap();

        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("collides with a subscription group")));
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        assert_eq!(
            out.get("proxy-groups")
                .unwrap()
                .as_sequence()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_rule_target_parsing_with_no_resolve_and_logic_rules() {
        assert_eq!(rule_target("MATCH,G"), Some("G"));
        assert_eq!(rule_target("DOMAIN-SUFFIX,x.com,argo-hk"), Some("argo-hk"));
        assert_eq!(
            rule_target("IP-CIDR,1.1.1.1/32,DIRECT,no-resolve"),
            Some("DIRECT")
        );
        assert_eq!(
            rule_target("AND,((NETWORK,udp),(DST-PORT,443)),REJECT"),
            Some("REJECT")
        );
        assert_eq!(
            rule_target("DOMAIN,x.com,argo-hk,no-resolve"),
            Some("argo-hk")
        );
        assert_eq!(rule_target("no-comma-rule"), None);
    }

    #[test]
    fn test_extra_rule_with_ip_rule_and_no_resolve_kept() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "mixed-port: 7897\n").unwrap();

        let extra_rules = vec![
            "IP-CIDR,198.18.0.0/16,DIRECT,no-resolve".to_string(),
            "DOMAIN-SUFFIX,x.com,missing-group".to_string(),
        ];

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &sub_nodes(),
            &sub_groups_fixture(),
            &serde_yaml::from_str("- MATCH,G").unwrap(),
            &extra_rules,
            None,
            &[],
        )
        .unwrap();

        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0].contains("missing-group"));
        let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
        let rule_list: Vec<&str> = out
            .get("rules")
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|r| r.as_str())
            .collect();
        assert_eq!(
            rule_list,
            vec!["IP-CIDR,198.18.0.0/16,DIRECT,no-resolve", "MATCH,G"]
        );
    }

    #[test]
    fn test_read_extra_groups_missing_file_and_empty_path() {
        assert!(read_extra_groups("").unwrap().is_empty());
        assert!(read_extra_groups("/nonexistent/extra-groups.yaml")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_read_extra_groups_parse_error_and_non_sequence() {
        let dir = TempDir::new().unwrap();
        let bad = dir.path().join("bad.yaml");
        std::fs::write(&bad, "invalid: [yaml\n").unwrap();
        assert!(read_extra_groups(bad.to_str().unwrap()).is_err());

        let map = dir.path().join("map.yaml");
        std::fs::write(&map, "name: x\ntype: fallback\n").unwrap();
        assert!(read_extra_groups(map.to_str().unwrap()).is_err());
    }
}
