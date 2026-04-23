Feature: rtb-vcs — provider registry (foundation slice)
  The v0.1 foundation exposes the trait + registry plumbing only.
  Backend-specific scenarios (GitHub, GitLab, Bitbucket, Gitea,
  Codeberg, Direct) land in their respective follow-up PRs. These
  scenarios exercise the cross-cutting registry + config behaviour
  that every backend inherits.

  Scenario: S-reg-1 — a registered mock factory is reachable via lookup
    Given the mock foundation backend is registered
    When I lookup the "mock-bdd-backend" source_type
    Then the factory is returned
    And the returned provider reports a release with tag "v1.0.0"

  Scenario: S-reg-2 — registered_types exposes every registration
    Given the mock foundation backend is registered
    When I list registered source types
    Then the list contains "mock-bdd-backend"
    And the list is sorted

  Scenario: S-reg-3 — looking up an unregistered source type returns None
    When I lookup the "ghost-backend" source_type
    Then the lookup returns None

  Scenario: S-reg-4 — Github config YAML round-trips through serde
    Given a Github config with host "api.github.com" owner "phpboyscout" repo "rust-tool-base"
    When I serialise then deserialise the config as YAML
    Then the resulting config matches the original
    And the discriminator is "github"

  Scenario: S-reg-5 — Codeberg config carries owner+repo but no host
    Given a Codeberg config with owner "phpboyscout" repo "rust-tool-base"
    When I inspect the Codeberg host constant
    Then the host constant is "codeberg.org"
    And the discriminator is "codeberg"

  Scenario: S-reg-6 — Custom config uses the source_type as its own discriminator
    Given a Custom config with source_type "internal-mirror"
    When I read the discriminator
    Then the discriminator is "internal-mirror"
