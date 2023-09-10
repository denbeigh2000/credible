# credible

[![built with nix](https://builtwithnix.org/badge.svg)](https://builtwithnix.org)

## What is this?
`credible` is a YAML-driven solution for storing, encrypting and retrieving secrets.

[`age`][age] is used for encryption/decryption.

Write your configuration:
```yaml
# credible.yaml
storage:
  type: S3
  bucket: my-secret-bucket  # S3 bucket name to use
  region: us-east-2         # Region of S3 bucket

secrets:
- name: "sample"    # Name of the secret
  encryption_keys:  # SSH public keys to encrypt with
  - ssh-ed25519 ...
  - ssh-ed25519 ...
  path: "sample"    # Path/key for backing object store

exposures:
- secret_name: sample   # Secret name to expose
  type: file            # Expose it as a file
  path: ./secret.txt    # Write the file to this path
- secret_name: sample
  type: env             # Expose it as an environment variable
  name: SAMPLE_SECRET   # Use this name
```

Upload your secret:
```
$ echo "hello world" | credible secret upload sample
```

Use it:
```
$ credible run-command -- sh -c 'echo $SAMPLE_SECRET; cat ./secret.txt'
hello world
hello world

$ echo $SAMPLE_SECRET; cat ./secret.txt

cat: ./secret.txt: No such file or directory
```

## Usage

### Using secrets
`credible` launches a program, provides secrets to it, and cleans them up when
the process has finished.

```
$ credible \
    --exposure env:sample:SAMPLE_SECRET \
    --exposure file:sample:./secret.txt \
    -- sh -c 'echo $SAMPLE_SECRET; cat ./secret.txt'
hello world
hello world

$ echo $SAMPLE_SECRET; cat ./secret.txt

cat: ./secret.txt: No such file or directory
```

Secrets can also be mounted in a tempfs for system-level access (will be unloaded on reboot)
```
# credible --expose file:sample:/etc/secret.txt system mount

# ls -l /run/credible/
total 4
-r-------- 1 root 12 Sep  9 14:42 secret.txt
[...]

cat /run/credible/secret.txt
hello world

cat /etc/secret.txt
hello world
```

### Configuration
`credible` aims to be a config-first, YAML-driven tool.

If no file is provided, `credible` looks for a `credible.yaml`/`credible.yml`:
```yaml
# credible.yaml
storage:
  type: S3
  bucket: my-secret-bucket  # S3 bucket name to use
  region: us-east-2         # Region of S3 bucket

secrets:
- name: "sample"        # Name of the secret
  encryption_keys:      # SSH public keys to encrypt with
  - ssh-ed25519 ...
  - ssh-ed25519 ...
  path: "sample"        # Path/key for backing object store

exposures:
- secret_name: sample       # Secret name to expose
  type: file                # Expose it as a file
  path: ./secret.txt   # Write the file to this path

- secret_name: sample       # Secret name to expose
  type: env                 # Expose it as an environment variable
  name: SAMPLE_SECRET       # Use this variable name
```

```
$ credible run-command -- sh -c 'echo $SAMPLE_SECRET; cat ./secret.txt'
hello world
hello world
```

---

You can dynamically configure secrets on the command line:

```
$ credible \
    --exposure file:sample:./super-secret.txt \
    --exposure env:sample:SUPER_SECRET \
    run-command sh -c 'echo $SUPER_SECRET; cat ./super-secret.txt'
hello world
hello world
```

---

Configuration is composable:

```yaml
# credible.secrets.yaml
storage:
  type: S3
  bucket: my-secret-bucket  # S3 bucket name to use
  region: us-east-2         # Region of S3 bucket

secrets:
- name: "sample"        # Name of the secret
  encryption_keys:      # SSH public keys to encrypt with
  - ssh-ed25519 ...
  - ssh-ed25519 ...
  path: "sample"        # Path/key for backing object store
```

```yaml
# credible.exposure.yaml
exposures:
- secret_name: sample
  type: file
  path: ./secret.txt
```

```
$ credible \
    --config-file ./credible.secrets.yaml \
    --config-file ./credible.exposure.yaml \
    --expose env:sample:TEST \
    run-command -- sh -c 'cat ./secret.txt; echo $TEST'
hello world
hello world
```

---

Errors are thrown on conflicting configuration:
```yaml
# credible.yaml
# ...

exposures:
- secret_name: sample
  type: file
  path: ./secret.txt
- secret_name: other_sample
  type: file
  path: ./secret.txt
```

```
$ ./target/debug/credible run-command -- sh
20:30:08 [ERROR] error: bad command line arguments: duplicate secret path specified: ./secret.txt
```

## Disclaimer

This project has received **NO** security auditing, and comes with no
guarantees or warranties of any kind, express or implied.

## Reporting/vulnerabilities etc.

Please report any security issues/vulnerabilities to `denbeigh (at) denbeigh stevens (dot) com`.

No big corporate bug bounty, but I may buy you a drink if you're local.

<!--

(for when the nix stuff is more polished)

### Nix integration

`credible` is a standalone tool that aims to provide an easy integration
experience for [Nix/NixOS][nix] users.

`credible` aims to provide user/system-level mounting similar to
[`agenix`][agenix] and [`sops-nix`][sops], but with the aim of _not_ committing
secrets to the nix store, and instead retrieving them at runtime.
This trades off some reproducibility of a system for other
benefits (easier rotation, access revokation, secrets are never outdated).

In addition, wrapping libraries are provided so `credible` can be easily used
in nix-managed tooling, as well as modules for [NixOS][nix], [`nix-darwin`] and
[`home-manager`].
-->


[age]: https://github.com/FiloSottile/age "age"
[agenix]: https://github.com/ryantm/agenix "agenix"
[gcs]: https://cloud.google.com/storage
[home-manager]: https://github.com/nix-community/home-manager "home-manager"
[nix-darwin]: https://github.com/LnL7/nix-darwin "nix-darwin"
[nix]: https://nixos.org "Nix/NixOS"
[s3]: https://aws.amazon.com/s3/
[sops]: https://github.com/Mic92/sops-nix "sops-nix"
