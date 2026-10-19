# harbor — a marina platform, deliberately polyglot

One product, five services. Each service is a COMPLETE application in its own
language and its own architectural style; nothing is shared between them.

| Service        | Language   | Architecture          | HTTP framework |
|----------------|-----------|-----------------------|----------------|
| berth-booking  | Java      | default layered       | Spring Boot    |
| billing        | Kotlin    | DDD (domain/application/infrastructure) | Spring Boot |
| tide-alerts    | Rust      | default modular       | axum           |
| search         | TypeScript| default layered       | express        |
| webhooks       | Python    | default layered       | Flask          |

The point this monorepo exists to prove: the FEATURE (an HTTP API) is
recognizable no matter which language serves it — routes, handler bodies,
decisions, throws and returns are extracted from every service the same way,
and grouped under the feature rather than the file suffix.
