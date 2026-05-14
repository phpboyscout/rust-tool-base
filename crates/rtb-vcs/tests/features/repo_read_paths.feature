Feature: rtb-vcs — Repo read paths (walk / diff / blame)
  v0.5 commits 2 and 2b. Downstream tools need to walk the commit
  graph (release-note summarisation), compare tree states
  (scaffolder regeneration drift detection), and attribute lines
  to commits (audit / code-review tools).

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

  Scenario: BL1 — blame attributes lines to the introducing commit
    Given a 3-commit linear-history fixture authored by "alice"
    When I blame "LICENSE" at "HEAD"
    Then every line is attributed to "alice"
    And every line maps to the commit at "HEAD~1"
