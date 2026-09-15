---
name: review-security
description: Review the mosquitto MQTT broker (C) and its dashboard (JS) for security issues across a scope using Semgrep (SAST) and Trivy (SCA/secrets/IaC), plus a manual C/JS pass.
temperature: 0.3
---

# Security Review — legacy-be-c (mosquitto)

Product: `mosquitto/` — a C implementation of an MQTT broker (its own git submodule
with upstream CI: CodeQL, ASan, Coverity, cifuzz). Dashboard: `mosquitto/dashboard/src`
(vanilla JS + Tailwind + vendored Chart.js, no `package.json`). Tooling lives at the
workspace level; never edit `mosquitto/.github` or other upstream files.

The deterministic gates are the contract: **Semgrep** (SAST) and **Trivy** (SCA +
secrets + IaC). A review is complete when both gates are green on the requested scope
and every finding carries a concrete fix.

## Scope Detection

| Input | Scope | Action |
|---|---|---|
| *(none)* | **uncommitted** | `git status --short`, `git diff`, `git diff --cached`, `git diff --stat` |
| 40-char SHA / short hash | **commit** | `git show <arg> --stat`, `git show <arg>` |
| branch name | **branch** | `git diff <arg>...HEAD --stat`, `git diff <arg>...HEAD` |
| contains `github.com` / `pull` / numeric | **PR** | `gh pr view <arg>`, `gh pr diff <arg>` |
| `all` / `--all` / `full` | **all codebase** | full scan of the mosquitto working tree |

`mosquitto/` is a submodule: for diff scopes run git inside `mosquitto/`
(`cd mosquitto && git diff ...`) unless the user means the workspace root.
Read the **full** changed files (not just the diff) before judging findings.

Context to read before reviewing:

- `docs/arch/security-architecture.md` — auth/authz model, fail-closed rules, NOGOs.
- `mosquitto/SECURITY.md` — upstream vulnerability reporting policy.
- `config/security/semgrep.yml`, `config/security/trivy.yaml`, `.semgrepignore`, `.trivyignore`.
- Shared security contract `A10/A11/A17/B7/B8/B9/D2/D3` (machine-checkable subset enforced below).
  Source of these codes: `platform-engineering/references/examples/security-requirements.md`
  under the initialize-workspace skill: `/Users/michal/.agents/skills/platform-engineering/references/examples/security-requirements.md`.

## Toolchain

| Layer | Tool | Invocation |
|---|---|---|
| 1 — SAST | Semgrep 1.177 (pipx) | `~/.local/bin/semgrep` (add `$HOME/.local/bin` to `PATH`) |
| 2 — SCA + secrets + IaC | Trivy 0.74 (native binary) | `trivy` (no Docker needed; `docker` here is podman) |
| 3 — Manual | grep/read patterns | see Layer 3 |

Run all commands from the workspace root `/Users/michal/workspace/ai-craft/legacy-be-c`.
If a binary is missing, note it and skip that layer — do not fake results.

## Layer 1: SAST — Semgrep

`mosquitto/` is a git submodule, so the workspace root repo does **not** track its
files; Semgrep must be told to descend with `--no-git-ignore`. Path filtering lives in
`.semgrepignore` (build artifacts, vendored `dashboard/src/lib`, non-runtime trees).

Gate (blocks on ERROR-level findings):

```bash
export PATH="$HOME/.local/bin:$PATH"
semgrep --config config/security/semgrep.yml \
        --config p/security-audit \
        --config p/secrets \
        --no-git-ignore \
        --severity ERROR \
        --exclude-rule c.lang.security.double-free.double-free \
        --error .
```

Full report (all severities, for context):

```bash
semgrep --config config/security/semgrep.yml --config p/security-audit --config p/secrets \
        --no-git-ignore --json --output /tmp/semgrep.json .
```

Parse `/tmp/semgrep.json`:

```bash
python3 -c "
import json,collections
d=json.load(open('/tmp/semgrep.json'))
for r in d['results']:
    print(r['extra']['severity'], r['check_id'], f\"{r['path']}:{r['start']['line']}\", '-', r['extra']['message'].splitlines()[0])
print('total:', len(d['results']))
"
```

Report `ERROR` and `WARNING` findings; never report `INFO`. Map high-signal C rules to
issues: `c.lang.security.*` (buffer/format/use-after-free), `*.insecure-use-*`
(bounded copies), `*.double-free`, and project rules `legacy-be-c.*`.

Accepted rule exclusions (documented in `config/security/semgrep.yml`):

- `c.lang.security.insecure-use-string-copy-fn` / `-strtok-fn` / `-strcat-fn` — WARNING
  level; bounded-buffer usage in this mature C codebase. Not gate-failing (`--severity
  ERROR`). Report only if the changed lines introduce an unbounded copy.
- `c.lang.security.double-free` — false positive in `libcommon/memory_common.c`: the two
  `free(mem)` calls sit in mutually exclusive `#if ALLOC_MARKER_SIZE` branches. Re-check
  this exclusion whenever `memory_common.c` changes.

A rule exclusion that no longer holds is itself a finding.

## Layer 2: SCA + Secrets + IaC — Trivy

Scan the whole tree (Vulns: `requirements.txt` + any manifests; Secrets: whole tree;
Misconfig: `mosquitto/docker/*/Dockerfile`, `mosquitto/security/*.apparmor`):

```bash
trivy fs --config config/security/trivy.yaml --format json --output /tmp/trivy.json .
```

`config/security/trivy.yaml` sets scanners (`vuln,secret,misconfig`), severity
(`HIGH,CRITICAL`), skip-dirs, and the ignorefile `.trivyignore`. Parse the three
categories separately:

```bash
python3 -c "
import json
d=json.load(open('/tmp/trivy.json'))
for r in d.get('Results',[]):
    for v in r.get('Vulnerabilities') or []:
        print('VULN', v['Severity'], v['PkgName'], v['InstalledVersion'], '->', v.get('FixedVersion'), v['VulnerabilityID'])
    for s in r.get('Secrets') or []:
        print('SECRET', r['Target'], s['RuleID'])
    for m in r.get('Misconfigurations') or []:
        print('MISCONF', m['Severity'], m['ID'], r['Target'])
"
```

Report format per category:

| Category | Line |
|---|---|
| Vuln | `{severity} {package} {installed} -> {fixed} {CVE} — {title}` |
| Secret | `{severity} {file}:{line} secret:{type} — {fix}` |
| Misconfig | `{severity} {file}:{line} misconfig:{rule} — {fix}` |

`.trivyignore` policy: entries are allowed **only with a justification comment**.
Currently `DS-0002` and `DS-0029` are ignored because they are findings in upstream
`mosquitto/docker/*` Dockerfiles (third-party, not owned here). Re-verify on every
submodule bump. Secrets are **never** ignored — they must be fixed. Image scan
(`trivy image`) only for `all` scope or when a Dockerfile changes.

## Layer 3: Manual Pattern Review

Grep the changed files, then read the full file to confirm each hit. Rules come from
`docs/arch/security-architecture.md` (fail-closed auth/authz) and the shared contract.

### C — broker and libraries

| # | Pattern | Rule source | Grep |
|---|---|---|---|
| C1 | Fail-open auth: `IGNORE`/`DEFER` treated as success | security-architecture NOGO | `grep -nE "MOSQ_ERR_PLUGIN_(IGNORE|DEFER)" <files>` — trace each result |
| C2 | Hardcoded credentials / keys in source | D2 | `grep -nE "(pass(word\|wd)?\|secret\|token\|api[_-]?key)\s*=\s*\"[^\"]+\"" <files>` |
| C3 | Command injection | OWASP | `grep -nE "\b(system\|popen\|execv\|execl)\s*\(" <files>` |
| C4 | Unbounded/string copies | Semgrep coverage | `grep -nE "\b(strcpy\|strcat\|sprintf)\s*\(" <files>` |
| C5 | Credentials/key material in logs | security-architecture | `grep -nE "MOSQ_LOG_\w+\(.*(password\|secret\|key\|credential)" <files>` |
| C6 | TLS client cert verification disabled | security-architecture | `grep -nE "SSL_VERIFY_NONE\|require_certificate\s*=\s*false" <files>` — verify it is intentional/config-driven |
| C7 | Unvalidated identity into ACL topic | security-architecture | `grep -nE "acl\|topic" <files>` — check NUL/wildcard rejection before pattern substitution |
| C8 | Anonymous access as auth fallback | security-architecture NOGO | `grep -nE "allow_anonymous" <files>` — only the shipped no-auth demo configs may enable it |

### JS/HTML — dashboard (`mosquitto/dashboard/src`)

| # | Pattern | Requirement | Grep |
|---|---|---|---|
| F1 | Raw HTML rendering (XSS) | B7 | `grep -nE "innerHTML\|outerHTML\|insertAdjacentHTML\|document\.write" mosquitto/dashboard/src/app` |
| F2 | Token in Web Storage/cookies | B9 | `grep -nE "localStorage\|sessionStorage\|document\.cookie" mosquitto/dashboard/src/app` (custom Semgrep rule covers token-like keys) |
| F3 | Secrets in frontend code | B8 | `grep -nE "(api[_-]?key\|token\|secret\|password)\s*[:=]\s*['\"]" mosquitto/dashboard/src` |
| F4 | Dynamic code execution | A10 | `grep -nE "\beval\(\|new Function\(" mosquitto/dashboard/src` |

`dashboard/src/lib/*` is vendored/minified — never report on it.

## Output Format

Dense — one line per finding, each with a concrete fix:

```
{severity} {file}:{line} {rule} — {one-line fix}
```

Example lines:

```
ERROR mosquitto/libcommon/memory_common.c:249 c.lang.security.double-free — FP: guarded #if branches; keep exclusion, re-verify on edit
WARN  mosquitto/src/handle_connect.c:1009 security-architecture — confirm use_identity_as_username requires require_certificate
HIGH  mosquitto/dashboard/src/app/listeners.js:42 F1 — build DOM via textContent/createElement, not innerHTML from API data
MED   mosquitto/docker/2.1-ubuntu/Dockerfile:34 misconfig:DS-0002 — upstream file; tracked in .trivyignore, not owned here
```

Group under `SAST` / `SCA-Vulns` / `SCA-Secrets` / `SCA-Misconfig` / `Manual`, omit
empty groups, end with a verdict line:

```
Verdict: FAIL — 2 findings in scope (1 HIGH, 1 MED); blocks merge until fixed
```

`PASS` only when the requested scope has zero in-scope findings and both gates exit 0.

## Rules

- **Submodule boundary**: `mosquitto/` is upstream; scan it, but never propose edits to
  `mosquitto/.github` or other upstream files. Findings inside upstream code are
  reported as context and, if accepted, justified in `.trivyignore`/the Semgrep config.
- **Scope discipline**: filter SAST/SCA findings to diff files for
  `uncommitted`/`commit`/`branch`/`PR`; full scan for `all`. Misconfigs describe
  deployment posture — always report them fully.
- **Severity**: never report Semgrep `INFO`; report `ERROR`/`WARNING`. Trivy gate is
  `HIGH,CRITICAL`; lower severities only in the manual pass.
- **Every finding carries a fix** — no finding line without a one-line remediation.
- **Skips are evidence**: every `--exclude-rule`, `.semgrepignore`, and `.trivyignore`
  entry needs a justification; an unjustified or stale skip is itself a finding.
- **Read full files**: never judge from a diff alone.
- **Fixtures vs production**: credentials in `pwfile.example`/`pskfile.example` and
  test fixtures are examples (note, downgrade); the same literal in runtime C is HIGH.
- **Gate green**: report both gate exit codes; a non-zero gate with no in-scope findings
  is still a FAIL (indicates a skip/config problem).
