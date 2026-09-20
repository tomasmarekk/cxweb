# Native arithmetic repair exercises

Status: **prepared; account-live execution NOT RUN**. These four exercises are
a subset of coding acceptance, not the complete 20-exercise G2 suite and not
additional protocol-corpus samples.

Version: cxweb.arithmetic-repairs.v1. Ordered specification SHA-256:
b073abc6cc040afaf32f3b72621c5aaf8ec73c07682a086ec774744e05291af5.

The fixed cases cover sum, inclusive interval count, absolute distance and ceiling
page count. Each starts with a function that fails its offline tests. Reference
solutions validate the fixtures themselves and are not sent to the model. The
model receives the problem and must read the actual function and cases through
Codex, run the failing tests, patch the return expression, rerun the tests and
acknowledge success only after the actual successful native result.

The fixture permits only a bounded ASCII expression over primitive numeric a/b
parameters and operators in an exact function wrapper. Strings, other names,
property access, arbitrary statements and additional function bodies are denied.
The trusted test runner revalidates source before evaluating it with primitive
inputs, disabled code generation and a deadline. This is not an arbitrary-code
sandbox. Different valid solutions can pass; approval does not compare against
the reference expression. Incorrect but admissible expressions fail the tests.

Only one attributed update to solve.cjs and the exact read/test commands can be
approved. The tests and test cases must remain unchanged. Ordered native item
results, actual exit codes, final file bytes and final-answer ordering determine
success. Local tests of this harness do not count as native client or account
execution. A live failure must be retained; rerunning a case is another attempt,
never a replacement for its first outcome.

Run local fixture and approval checks:

    node --test scripts/coding-fixture.test.mjs scripts/coding-fixture-approval.test.mjs scripts/probe-client-approval.test.mjs

Run one explicit installed-client exercise only when the browser account is idle:

    node scripts/probe-installed-client.mjs <reviewed-client-exe> <absolute-native-home> <owned-model> --coding=sum

Other IDs: inclusive-count, absolute-distance, ceiling-pages. Use each reviewed
native client separately. The existing probe binds reviewed executable hashes,
uses installed routing without config overrides and stores content-free results
in its unique local workspace. Native source/test artifacts are synthetic fixture
data. Broader sequence journaling, cancellation/concurrency exercises and the
remaining coding scenarios still require separate acceptance work.

Local validation: all 15 fixture/approval tests passed, including a real local
Node process exiting 1 before repair, 0 after repair and 2 for rejected source.
Native App/CLI tools have not yet executed these four new cases.
