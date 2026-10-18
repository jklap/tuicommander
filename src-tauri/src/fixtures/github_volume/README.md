# Controlled GitHub client measurement

These files are test-only TLS material for `measure_github_client_fixture_1995`.
The private key is public fixture data, not a GitHub credential. The test uses a
fake token, disables proxy discovery, trusts only this certificate and resolves
`api.github.com` to its loopback TLS receiver. It does not access a live account.

The API bodies are controlled protocol inputs, not captured GitHub responses.
The receiver records requests sent by the production poller and HTTP client.
The workload selects one hot repository, one account and a visible app. It
replays the open/reopen checks calls observed in the separate frontend trace;
it does not drive a browser and Rust backend together.

Run only the explicitly ignored experiment, through the required build admission:

```sh
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml \
  --lib -E 'test(measure_github_client_fixture_1995)' --run-ignored all \
  --success-output immediate
```

The default Nextest profile has a test-specific 360-second outer bound, above
the experiment's 300-second hang bound. Routine tests keep their existing bound.
The experiment deliberately waits through two real 60-second poll windows.
It prints one `GITHUB_VOLUME_1995` JSON record with receiver counts and queries.

The certificate was generated with:

```sh
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout key.pem -out cert.pem -subj /CN=api.github.com \
  -addext subjectAltName=DNS:api.github.com \
  -addext basicConstraints=critical,CA:FALSE -days 3650
```

Counts do not establish GraphQL point cost, secondary-limit safety, a whole-app
upper bound, or the cause of a historical blocked tab. The controlled rate-limit
cases prove zero receiver sends on an immediate retry after exhausted 403/429,
then one send after `retry-after: 1` expires, for REST and GraphQL separately.
