# Prompt 01: Ideation to Initial Specification

**Target Model:** Frontier Model A (Claude 3.7 Sonnet / GPT-4.5)  
**Output Artifact:** `doc/001_initial_spec.md`

---

## Prompt Template

```text
You are a Principal Systems Architect. I want to discuss a new software product/subsystem idea and formulate a formal, unambiguous Initial Specification.

MY PRODUCT IDEA & CONTEXT:
<Insert raw brainstorm, notes, desired language (Rust, C11, C++20, Java), target domain (Safety/Automotive, Enterprise Backend, Embedded, CLI), and goals here>

YOUR TASK:
Synthesize our discussion into a comprehensive `doc/001_initial_spec.md` document adhering to the following structure:

---
title: Initial Product Specification: <Product Name>
status: draft
version: 0.1
author: <Author / Model Name>
date: <YYYY-MM-DD>
---

# 001: Initial Product Specification - <Product Name>

## 1. Executive Summary & Problem Statement
- What core problem does this solve?
- Who are the target users / consuming systems?
- What are the primary success metrics?

## 2. Core Capabilities & Functional Scope
- Feature 1: Description and expected behavior
- Feature 2: Description and expected behavior
- ...

## 3. Non-Functional & Quality Requirements (ISO/IEC 25010:2023)
Explicitly address each of the 9 axes (classify as Required / Recommended / N/A with rationale):
1. Functional Suitability
2. Performance Efficiency (Latency, throughput, memory budget)
3. Compatibility (OS, interfaces, dependencies)
4. Interaction Capability (API ergonomics / CLI flags)
5. Reliability (Error recovery, fault isolation)
6. Security (Auth, input validation, memory safety)
7. Maintainability (Modularity, testability)
8. Flexibility (Extensibility, portability)
9. Safety (Fail-safe defaults, hazard mitigation - crucial for embedded/automotive)

## 4. Architectural Hypotheses & Tech Stack
- Proposed language: (e.g. Rust 2021, C11, C++20, Java 21)
- Key third-party dependencies / zero-dependency policy
- Concurrency & memory management model
- Protocol & data serialization formats

## 5. Scope & Explicit Non-Goals
- **IN SCOPE:** Explicitly listed features for Sprint 1 / MVP
- **EXPLICIT NON-GOALS:** Features deliberately excluded to prevent scope creep

## 6. Open Questions & Technical Risks
- Risk 1: ...
- Question 2: ...
```
