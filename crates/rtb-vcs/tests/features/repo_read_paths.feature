Feature: rtb-vcs — Repo read paths (walk / diff)
  v0.5 commit 2. Downstream tools need to walk the commit graph
  (release-note summarisation) and compare tree states (scaffolder
  regeneration drift detection). Blame ships separately in commit
  2b (see spec §8) because gix 0.72 exposes blame only via its raw
  re-export — splitting it out keeps this commit focused on
  gix::Repository-method APIs.

  Scenario: W1 — walk(HEAD) yields commits newest-first
    Given a 3-commit linear-history fixture authored by "alice"
    When I walk "HEAD"
    Then the walked commit messages are "third", "second", "initial" in order

  Scenario: W2 — walk(range) excludes commits before the lower bound
    Given a 3-commit linear-history fixture authored by "alice"
    When I walk "HEAD~2..HEAD"
    Then the walked commit messages are "third", "second" in order

  Scenario: W3 — walk(bad-revspec) surfaces RevspecNotFound
    Given a 3-commit linear-history fixture authored by "alice"
    When I attempt to walk "no-such-rev"
    Then the call fails with RepoError::RevspecNotFound for "no-such-rev"

  Scenario: D1 — diff between two commits surfaces structured changes
    Given a 3-commit linear-history fixture authored by "alice"
    When I diff "HEAD~2" and "HEAD~1"
    Then the diff contains "README.md" as Modified
    And the diff contains "LICENSE" as Added

  Scenario: D2 — diff captures deletions
    Given a 3-commit linear-history fixture authored by "alice"
    When I diff "HEAD~1" and "HEAD"
    Then the diff contains "README.md" as Deleted
    And the diff contains "CHANGELOG.md" as Added
