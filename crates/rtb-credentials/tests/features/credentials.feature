Feature: rtb-credentials — credential stores and precedence resolution
  As a framework consumer
  I want a typed, pluggable credential pipeline
  So that secrets never leak through untyped accessors

  Scenario: S1 — LiteralStore round-trip
    Given a LiteralStore containing "abc123"
    When I get any key
    Then the secret exposes as "abc123"
    And the Debug rendering redacts "abc123"

  Scenario: S2 — MemoryStore set-then-get
    Given an empty MemoryStore
    When I set "svc"/"acct" to "secret-value"
    Then getting "svc"/"acct" returns "secret-value"

  Scenario: S3 — Resolver prefers env over literal
    Given an empty MemoryStore
    And the environment variable "RTBCRED_BDD_S3" set to "env-wins"
    And a CredentialRef with env "RTBCRED_BDD_S3" and literal "literal-loses"
    When I resolve the reference
    Then the resolved secret is "env-wins"

  Scenario: S4 — Resolver prefers keychain over literal when env is unset
    Given a MemoryStore with "svc"/"acct" = "keychain-wins"
    And a CredentialRef with keychain "svc"/"acct" and literal "literal-loses"
    When I resolve the reference
    Then the resolved secret is "keychain-wins"

  Scenario: S5 — Missing credential surfaces NotFound
    Given an empty MemoryStore
    And an empty CredentialRef
    When I resolve the reference and capture the error
    Then the error is a NotFound variant

  Scenario: S6 — Literal refused under CI
    Given an empty MemoryStore
    And the environment variable "CI" set to "true"
    And a CredentialRef with only a literal "ci-leak"
    When I resolve the reference and capture the error
    Then the error is a LiteralRefusedInCi variant
