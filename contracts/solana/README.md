# QPC Solana Program Boundary

This is a separate Solana implementation boundary. It must be implemented in Rust (preferably Anchor only after its version and security model are pinned). It must not import or reuse Solidity/EVM code.

Required review areas: signer and owner constraints, PDA seeds/bump validation, account ownership, writable/signer flags, instruction data validation, CPI allowlists, replay protection, rent/close authority, compute limits and upgrade authority.

Status: architecture boundary only; no Solana program is implemented yet.
