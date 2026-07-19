# Privacy defaults

MeowEngine v0.2-alpha has no telemetry, analytics, account, cloud sync, remote
crash upload, ads, or background updater. Requests are made only for user-
requested pages and discovered resources.

Cookies/cache live in the network process. Local Storage persists only with a
profile; Session Storage does not. There is no private-browsing guarantee,
cookie partitioning, secure deletion, history encryption, or automatic expiry.

Inspector snapshots and diagnostics bundles require explicit F12/CLI action and
may contain URLs, DOM text, console, network errors, and crash data. Review them
before sharing. Environment values are excluded by default; selected key names
and tool/system versions are recorded.
