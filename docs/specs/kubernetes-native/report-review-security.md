# Review report — Security

- Scope: uncommitted changes (mosquitto submodule new `src/otel_metrics.*`, `src/sys_tree.*`, `src/conf.c`, `src/mosquitto.c`, build files; workspace `demo/`)
- Method: `/Users/michal/.agents/skills/review/references/review-security-instructions.md`; `mosquitto/docs/../docs/arch/security-architecture.md` (workspace `docs/arch/security-architecture.md`)
- Evidence base: full read of `mosquitto/src/otel_metrics.c|.h`, `src/sys_tree.c|.h`, `git diff` of `src/conf.c`, `src/mosquitto.c`, `src/CMakeLists.txt`, `make/broker.mk`, `config.mk`; Semgrep `p/security-audit`+`config/security/semgrep.yml`+`p/secrets`; Trivy `fs --config config/security/trivy.yaml` (output `.agents/tmp/trivy-out.txt`)

## Verdict
No HIGH/CRITICAL security defect in the new C code; Semgrep (ERROR) and Trivy (HIGH/CRITICAL, secret) are clean for `mosquitto/src`. One Medium availability/DoS-hardening gap (unbounded collector response buffering) plus low-severity hardening items. The egress target is admin-controlled configuration, not untrusted input.

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| S1 | Medium | HTTP response body is buffered with no size bound; a compromised/misbehaving collector can drive unbounded heap growth (broker OOM), and the body is then fed to `cJSON_Parse`. | `mosquitto/src/otel_metrics.c:395-409` (write cb reallocs `resp->len + n + 1` unconditionally), parse at `mosquitto/src/otel_metrics.c:517` | Cap buffered bytes (e.g. reject/stop above a small fixed limit, return 0 to abort transfer); optionally limit via `CURLOPT_MAXFILESIZE_LARGE`. |
| S2 | Low | No libcurl protocol restriction and `otlp_endpoint` is not validated to `http`/`https`; any scheme accepted by libcurl (e.g. `file`, `ftp`) is reachable if config is typo'd/injected. | `mosquitto/src/otel_metrics.c:484` (CURLOPT_URL only), `mosquitto/src/conf.c:2347` | Set `CURLOPT_PROTOCOLS_STR "http,https"` (and `CURLOPT_REDIR_PROTOCOLS_STR`) and validate the endpoint scheme at config parse. |
| S3 | Low | `otlp_headers` key/value are built into a header line with no CR/LF or control-char validation; a value containing CR/LF could inject/split headers. Config is a trusted, admin-owned file, and a literal newline cannot appear in a parsed line. | `mosquitto/src/otel_metrics.c:432-446`, `mosquitto/src/conf.c:2358-2363` | Reject keys/values containing CR, LF or control chars; also enforce a length bound (this path bypasses `conf__parse_string`'s `UINT16_MAX` + UTF-8 checks at `mosquitto/src/conf.c:3208`). |
| S4 | Low | Endpoint sanitiser strips only `user:pass@` userinfo; a credential placed in a query string or fragment would still be written to the log line. | `mosquitto/src/otel_metrics.c:652-673`, logged at `mosquitto/src/otel_metrics.c:724-726` | Also redact query/fragment, or document that credentials must go in `otlp_headers`/userinfo only. |
| S5 | Info | SAST WARNING `c.lang.security.insecure-use-string-copy-fn` (`strcpy`) — bounded (`memcpy` length + fixed suffix, buffer sized accordingly), consistent with the repo's accepted-risk note. | `mosquitto/src/otel_metrics.c:427`; `config/security/semgrep.yml` (accepted-risk comment) | No action; keep the explicit bounds comment. |
| S6 | Info | Demo manifest carries a hardcoded Basic credential in an OpenObserve exporter header. Dev/k3s demo fixture, not product code; Trivy secret scan reported 0 secrets. | `demo/k3s/k8s/10-otel-collector.yaml:30` | Rotate/parameterise before any non-demo reuse; leave as documented dev fixture. |
| S7 | Info | Trivy reports HIGH IaC misconfigs (`KSV-0118`, default security context) across `demo/k3s/k8s/*.yaml` (collector, jaeger, openobserve, mosquitto, postgres, settings-service). Deployment-scope, not src. | `.agents/tmp/trivy-out.txt` (KSV-0118 entries) | Add a pod/container `securityContext` to the k3s manifests. |

## Good practices / strengths
- Credential handling: the `otlp_headers` value (bearer token) is never logged — only the sanitised endpoint is logged (`mosquitto/src/otel_metrics.c:724-726`); parse errors log the key name only (`mosquitto/src/conf.c:2362`).
- TLS verification is on by default; code does not disable `CURLOPT_SSL_VERIFYPEER`/`VERIFYHOST` (so libcurl defaults apply), and the spec scopes external/TLS exposure as deferred (`docs/specs/kubernetes-native/spec.md:359,363`).
- Redirects are not followed (`CURLOPT_FOLLOWLOCATION` unset) and total request time is bounded by `CURLOPT_TIMEOUT` with `CURLOPT_NOSIGNAL` (`mosquitto/src/otel_metrics.c:488-489`).
- Config parsing bounds: `otlp_endpoint` uses `conf__parse_string` (UTF-8 validation + `UINT16_MAX` length, `mosquitto/src/conf.c:3208-3230`); intervals/timeouts validated `>= 1` (`mosquitto/src/conf.c:2354,2369`).
- Untrusted collector JSON parsed with cJSON (hardened parser) + explicit type checks before `strtol` (`mosquitto/src/otel_metrics.c:508-529`); no format-string or buffer issues found.
- Endpoint userinfo is stripped before logging (`mosquitto/src/otel_metrics.c:652-673`).
- SAST/SCA gates clean for `mosquitto/src` (Semgrep ERROR + Trivy HIGH/CRITICAL/secret).

## Open questions / unverified
- S3 exploitability unverified: depends on whether a CR can survive `mosquitto_trimblanks` from a CRLF config file and on libcurl's current header handling. Marked low/theoretical; config is trusted.
- No custom CA / client-certificate support for an HTTPS collector (spec defers external TLS). Unverified whether an operator needs mTLS to the collector.
- The `.semgrepignore`/`config/security/semgrep.yml` scan was invoked on the four changed product files only; full-tree Semgrep run not performed for this report.
