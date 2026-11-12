## About this project

This project was originally implemented in C, and there can be some assumptions
which does not align with Rust safety semantics. Specificaly,

1. some raw pointer lifetimes are manually reasoned. e.g. it's possible some
   Weak pointer is always guaranteed to be alive.
2. destruction should be kept logically reasoned. Each free operation must have
   exactly one responsible entity: either a dedicated free function or Drop,
   never both for the same resource. Drop is the long-term goal. While a
   resource still has a dedicated free function, keep the guarantee that it is
   called and do not also release that resource in Drop. Migrating a resource
   to Drop is welcome, as long as the migration is complete: move the whole
   release into Drop and remove the free function in the same change.

## Coding conventions

- Do not add line comments unless explicitly asked. Let the code speak itself.

## General software design

Generally these are preferrable design, but in some cases we should
acknowledge. 

- Prefer single ownership, Drop cleanup, reconstructible state, and minimal
  abstractions - DO NOT introduce Rc unless strictly necessary.
- Do not make overlapping design: When you add a new API, existing APIs should
  not be able to support the new API. If the new API can replace existing APIs,
  then all usages should be migrated to the new API and remove old API. For a
  single use case, there should be no two APIs supporting it.


