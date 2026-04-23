Feature: rtb-vcs — GitHub backend
  Tool authors pointing at a GitHub source get the spec-defined shape
  of releases, and the backend maps well-known failures (401, rate
  limit, 404 for drafts) to the right `ProviderError` variants.

  Scenario: S1 — happy path against a GitHub public repo
    Given a wiremock GitHub serving a release tagged "v0.1.0"
    When the updater asks for the latest release
    Then the returned tag is "v0.1.0"
    And the returned release has at least one asset

  Scenario: S7 — unauthenticated caller cannot see a draft release
    Given a wiremock GitHub where tag "draft-v0.2.0" returns 404
    When the updater asks for the "draft-v0.2.0" release without a token
    Then the returned error is NotFound
