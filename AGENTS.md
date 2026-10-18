## About this project

This project was originally implemented in C, and there can be some assumptions
which does not align with Rust safety semantics. Specificaly,

1. all object lifetime is logically reasoned manually. e.g. it's possible some
   Weak pointer is always guaranteed to be alive.
2. destruction should be kept logically reasoned. e.g. if there's some
   dedicated free function, we should keep the guarantee it being called until
   we migrate everything to drop function. Also while such manual Drop exists,
   we should not rely on Drop for resource management.

## General software design

Generally these are preferrable design, but in some cases we should
acknowledge. 

- Prefer single ownership, Drop cleanup, reconstructible state, and minimal
  abstractions - DO NOT introduce Rc unless strictly necessary.
- Do not make overlapping design: When you add a new API, existing APIs should
  not be able to support the new API. If the new API can replace existing APIs,
  then all usages should be migrated to the new API and remove old API. For a
  single use case, there should be no two APIs supporting it.


