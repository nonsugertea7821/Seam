# Seam Language Component Specification

## 1. Design Principles

In Seam, a program explicitly separates **data structures, state, operations, execution paths, and temporal control**, expressing each as a distinct type.

In traditional languages, functions ambiguously contained value transformations, state changes, and control flow boundaries within themselves.
In contrast, Seam isolates each responsibility into distinct language components, enabling **compile-time verification of execution paths and state transitions**.

### Overview of Components

| Domain | Component | Role | Ownership / Reference |
| --- | --- | --- | --- |
| **Value** | `primitive` | Minimal value represented directly on the CPU/ABI | None (Direct value) |
|  | `record` | Immutable data structure combining multiple values | None (Passed by copy) |
| **State** | `resource` | Target whose state changes over time (reference) | Present (Reference / Static analysis) |
| **Operation** | `operator` | Unit of execution performing state transitions and value transformations | ABI expansion (Physical layer boundary) |
| **Path** | `channel` | Execution path combining multiple operators / controls | Stateless (Pure path) |
| **Control (Time/Control)** | `control` | Sequencing, iteration, and parallelization of execution paths (temporal structure) | Contains no execution logic itself |

---

## 2. primitive

A `primitive` is the minimal representation of a value in Seam.

### Characteristics

* **Direct Representation:** Represents a value that can be represented directly on the CPU or ABI, holding no references to other data or state.
* **Elimination of Pointers:** Seam does not treat pointers as `primitive`s; all reference concepts are decoupled and isolated into `resource`s.
* **Stateless & Lifecycle-Free:** Because it represents the value itself, it carries no concept of ownership or lifecycle. Copying is always safe and requires no additional management across `channel` or `operator` boundaries.

### Example Definition

```seam
int value;
bool result;

```

> **Note:** Types like `int` and `bool` carry value representations only and hold no association with external state.

---

## 3. record

A `record` is an **immutable value-type data structure** composed of `primitive`s and other `record`s.

### Characteristics

* **Immutability:** Represents a collection of information, not a stateful container. Internal fields cannot be mutated after instantiation.
* **Update Model:** Updating a `record` does not mutate existing data; instead, it is expressed as the **instantiation of a new `record**`.
* **Safe Sharing and Copying:** Can be safely shared across multiple execution paths. However, crossing a `channel` boundary treats it strictly as a value, incurring a **memory copy**. This serves as Seam's fundamental boundary to prevent state contention caused by shared references.

```text
old record
    +
changed value
    ↓
new record

```

### Example Definition

```seam
record User {
    int id;
    string name;
}

```

> **Compiler Optimization & Performance Model:**
> While crossing a `channel` boundary semantically treats a `record` strictly as a value (conceptually incurring a copy), the compiler analyzes the enclosing `control` structure to safely optimize this boundary.
> * **Zero-Copy & CoW (Copy-on-Write):** Read-only passes or unmutated values are automatically lowered to pointer borrowing or Copy-on-Write without mutating semantics.
> * **Performance Tuning Strategy:** If further performance optimization is required for massive datasets, developers are encouraged to re-architect data domain layouts (e.g., splitting `record` fields) or re-evaluate `control` flows, rather than introducing complex pointer/reference mechanics into values.
> 
> 

---

## 4. resource

A `resource` is a **reference type** that represents an entity whose state changes over time.

### Characteristics

* **Hidden References:** Under the hood, it is represented as a pointer reference; however, Seam forbids raw pointers and safely manages everything as a `resource`.
* **Targets:** Represents external or physical entities that cannot be copied as values, such as:
* Shared memory
* Hardware state
* Files / Network connections
* OS syscalls / External services


* **Static Analysis:** Because operations on resources can introduce side effects, they are subject to static analysis by the compiler.

> **Design Contrast:**
> * `record`: Expresses "what it is" (static information).
> * `resource`: Expresses "the currently existing state" (dynamic entity).
> 
> 

### Example Definition

```seam
resource File {
    int fd;
}

```

---

### resource contract

When a `channel` accesses a shared `resource`, it declares its access semantics as a contract.

```seam
requires {
    read {
        File.fd;
    }
    write {
        File.fd;
    }
}

```

* **Usage as Effect Information:** Rather than acting solely as access control, contracts serve as effect information along the execution path.
* **Race Condition Verification:** The compiler analyzes execution paths generated by `channel`s and `fork`s, statically checking for data races or potential state inconsistencies on `resource`s.

> **Architectural Clarification (Channel vs. Resource):**
> A `channel` is purely a stateless **execution path**, not a state-bearing container, object, or class. Therefore, `channel`s do not "own" `resource`s, nor do they require dynamic lifecycle/ownership tracking (such as GC or manual destructors). All state interaction along a path is strictly declared, verified, and arbitrated via `operator` `requires contract` (effect system).

---

## 5. operator

An `operator` is the **minimal unit of execution** in Seam.

It represents value transformations or operations on `resource`s. While corresponding to operators like `+` or `==` in traditional languages, an `operator` is not merely syntactic sugar—it is an **execution definition directly lowered into ABI instructions**.

> **Comparison with `channel`:**
> * `channel`: Defines "**along which path** processing occurs."
> * `operator`: Defines "**what state transition occurs along that path**" (*it can also be viewed as a channel without a collector*).
> 
> 

### Direct ABI Binding

An `operator`'s implementation is determined by ABI definitions and lowered by the compiler into corresponding ABI instructions. It establishes the boundary between language abstractions and hardware capabilities.

```seam
operator == {
    left int;
    right int;

    return bool;

    abi {
        x86_64 {
            cmp left, right;
            sete return;
        }
    }
}

operator add {
    left int;
    right int;

    return int;

    abi {
        x86_64 {
            add left, right;
        }
    }
}

```

---

### Operators and Destructiveness (Effect Guarantee & Concurrency)

* **Standard Operators:** For operators whose semantics are standardly defined by Seam, the compiler can analyze their effects.
* **Custom / Undefined Operators:** For operators not built into Seam or defined by users with custom ABI implementations, the compiler cannot guarantee their internal behavior. In such cases, the operator is treated as a **destructive operation**.

> **Fundamental Rule:** Handled under a simple principle: "Any operation whose semantics are not defined by Seam is unverified (treated as destructive)."

### Classification (`safe` / `unsafe`) and 2PST Execution Model

To handle custom/unverified operators during concurrent execution without rejecting user code:

1. **Classification:** Operators are categorized as `safe` (read-only / pure value transformations) or `unsafe` (write / state mutations / custom unverified ABI execution).
2. **2PST (2-Phase Static Transaction) Negotiation:** An `unsafe` operator is **never rejected or forbidden** from parallel execution (`fork`). Instead, the compiler simply waives full static race-free verification and automatically wraps the execution in a **2PST (2-Phase Static Transaction)** framework.
3. **Runtime Guarantee:** During `fork` execution, the compiler inserts a static Prepare/Commit transaction boundary around `unsafe` operators, ensuring atomic state transitions and data consistency at runtime even when static effect verification is waived.

---

## 6. channel

A `channel` is the **unit of execution paths** in Seam.

While serving a role similar to functions in other languages, it carries no concept of instances or references. It represents the callable "execution path itself," rather than a state-bearing object.

```seam
channel Process {
    entry {
        // Normal path processing
    }
    collector {
        // Error handling / recovery path
    }
}

```

---

### entry (Normal Execution Path)

* Represents the normal execution path of a `channel`. Evaluation typically begins at `entry` upon invocation.
* Evaluation terminates in one of two ways:
* `return value;` : Normal completion
* `abort value;` : Unrecoverable execution state; yields control of the current execution path to the `collector`



---

### collector (Error Recovery Path)

* Represents the error recovery path of a `channel`. Exception handling is not performed via dynamic lookup.
* Statically bound at call sites using `:collect`.

```seam
Child() :collect GrandChild;

```

* When an `abort` occurs, the runtime directly transitions to the pre-determined `collector`. **No stack unwinding or handler searching ever occurs.**

---

### local resource

A local resource accessible exclusively to a specific `channel` can be defined within that `channel`.

```seam
channel Worker {
    resource LocalState {
        // State isolated from the outside world
    }
}

```

* Because it cannot be referenced by external `channel`s, the compiler can guarantee the absence of external data races.
* Consequently, it does not require `requires` contracts like shared resources do.

---

## 7. control

A `control` represents the **temporal structure of execution paths**.

It performs no value transformations or state mutations itself; instead, it defines the order in which `channel`s and `operator`s are evaluated.

**Target Structures:** `if` / `switch` / `loop` / `fork`

---

### selector

`if` and `switch` determine paths via a `selector`.

A `selector` is not merely a conditional expression; it is treated as a `channel` or `operator` that returns a `bool` or `enum`.

```text
selector
    ↓
control
    ↓
selected path

```

---

### fork (Generating Concurrent Paths)

`fork` is a `control` that spawns multiple execution paths. Rather than selecting between branches, it allows **multiple paths to exist simultaneously**.

```text
fork
 ├─ path A
 └─ path B

```

Concurrent paths created by `fork` are statically analyzed at compile time using `resource contract`s to verify data race freedom and synchronization conditions.

> **Execution Guarantee under `fork`:**
> When a `fork` encounters paths containing verified `safe` operators, it executes them with zero synchronization overhead. When paths contain custom or unverified (`unsafe`) operators, `control` automatically arbitrates execution via **2PST (2-Phase Static Transactions)**, guaranteeing memory and state safety without rejecting parallel execution.

---

## 8. Architectural Model

```text
Syntax
  │
Compiler
  │
  ├─► primitive  ───┐
  ├─► record     ───┼─► (Value & State Definitions)
  ├─► resource   ───┘
  │
  ├─► channel    ───┐
  ├─► control    ───┼─► (Path & Time Definitions)
  │                 │
  └─► operator ─────┘
            │
            ▼
           IR
            │
            ▼
         Runtime
            │
            ▼
           ABI
            │
            ▼
       OS / Hardware

```

### Summary

In Seam, the **`operator` anchors the interface to the physical layer**, the **`channel` guarantees its execution path**, and the **`control` shapes time and structure**.

Through this explicit separation, distinct concepts—value, state, operation, path, and time—are never conflated, enabling precise compile-time verification of safe execution boundaries.