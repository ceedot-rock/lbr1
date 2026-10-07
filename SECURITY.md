# Security Policy

PCC is a codec: a file you decode must be exactly the file someone encoded.
Any integrity or verification weakness is a security issue, not a normal bug.

## Reporting a vulnerability

Please do not open a public issue for security problems.

- Email: corey@slidphilabs.com with the subject line `LBR1 security`
- Or use GitHub's private vulnerability reporting on this repository
  (Security tab, "Report a vulnerability")

Include the affected file or module, steps or inputs to reproduce, and what
you expected versus what happened.

You can expect an acknowledgement within 3 business days. We will keep you
updated while we investigate and credit you unless you prefer to stay
anonymous.

## In scope

- Decode mismatch: decoded output differs from the original input
- `lb test` / CRC verification bypass: a corrupt or tampered file passes as
  `DECODE_OK`
- Frame / archive (`lb zip`) integrity: a tampered `.pcc` unpacks without error
- Private encoder, signing-key, or AUTH_DIR material exposed anywhere in the
  public tree

## Out of scope

- Benchmarks you disagree with (file an issue with measured numbers)
- Operator deployments we do not run
- Social engineering, spam, or denial-of-service against hosted demos
