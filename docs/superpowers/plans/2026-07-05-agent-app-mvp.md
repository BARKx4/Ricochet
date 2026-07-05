# Cross-Platform Agent App MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a polished Windows/Linux desktop agent app that proves Ricochet can compose UI, local process/PTY workflows, approvals, and packaging with less glue code than a conventional stack.

**Non-Goals:**

- No new native renderer.
- No broad agent framework.
- No cloud dependency for the core demo.
- No unregistered public words.

## MVP Shape

- WebView app-kit shell with sidebar, toolbar, task list, log/output pane, approvals pane, and status bar.
- Local workspace root and process root configured explicitly.
- One agent workflow that reads a repo, proposes a small change, requests approval, runs a command, and reports result.
- Packaged Windows app and Linux embedded WebView app.
