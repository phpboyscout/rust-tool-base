# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities via GitHub's private vulnerability
reporting on this repository, or by email to `security@phpboyscout.uk`.

Please do **not** open a public issue for security-related matters.

## Supported Versions

While `rust-tool-base` is pre-1.0 (`0.x`), security fixes are released
against the latest `0.y` line only. After 1.0, security support will cover
the current and one previous minor line.

## Scope

`rust-tool-base` distributes a framework crate and a companion CLI (`rtb`).
Both are in scope. Downstream tools built on the framework are out of scope
— report those to the tool maintainer.
