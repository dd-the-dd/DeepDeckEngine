# `agent-manifest/v1`

`schema.json` is the published JSON Schema for the agent registration metadata owned by
DeepDeckEngine. Consumers should validate against this schema and serialize camel-case wire
keys. Unknown fields are not part of the contract.

The fixtures define three compatibility cases:

- `minimal.json` proves older manifests without `agentType`, publications, or capabilities
  continue to load with defaults;
- `maximal.json` is shared byte-for-byte with the Python SDK and DeepDeckLearner;
- `invalid.json` parses as JSON but fails the manifest's semantic author validation.

The contract version in the directory, schema ID, `schemaVersion` constant, and Rust
`AGENT_MANIFEST_VERSION` are checked together by `tests/agent_manifest_contract.rs`.
