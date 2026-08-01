# Build on a PC and Deploy to a Small VPS

This guide is for an Ubuntu VPS that has enough memory to run TrustTunnel but
not enough memory to compile it. A 512 MB VPS is a typical example. The server
is built from the reviewed repository on an Ubuntu VM on your PC, packaged
outside the source tree, verified, and then transferred with `scp` or a GitHub
Release asset.

The release archive contains the two executables, the systemd template,
licenses, and plain-text source/toolchain provenance. It must never contain
endpoint configuration, credentials, certificates, client profiles, or
private keys.

## Choose a compatible build VM

Native Linux executables depend on CPU architecture and the system C library.
The least surprising setup is:

- Ubuntu VM and VPS use the same Ubuntu release, and the VM's glibc version is
  not newer than the VPS version;
- both use the same architecture, such as `amd64` or `arm64`;
- the VM has at least 4 GB RAM and enough free disk for Rust and native
  dependency builds;
- the VM is trusted with source code and build credentials, but not with live
  VPN credentials.

On the VPS and VM, compare:

```bash
cat /etc/os-release
dpkg --print-architecture
uname -m
ldd --version | head -n 1
```

An `amd64` binary does not run on `arm64`, and a binary built against a newer
glibc can fail on an older VPS with a `GLIBC_x.y not found` error. Matching the
Ubuntu release is necessary but not sufficient when the VM has newer package
updates, so compare the complete `ldd` version lines and build against the
older one. Cross-compilation is possible but is outside this baseline because
native TLS dependencies make it easier to produce an artifact that was not
actually tested for the target.

## Prepare the Ubuntu build VM

Install native build prerequisites:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends \
    build-essential ca-certificates clang cmake curl file git less libclang-dev \
    make pkg-config python3 python3-venv
```

Rust's official installation method is `rustup`. Download the installer first
so it can be inspected or compared with your organization's approved copy:

```bash
curl --proto '=https' --tlsv1.2 --fail --show-error \
    https://sh.rustup.rs -o /tmp/rustup-init.sh
less /tmp/rustup-init.sh
sh /tmp/rustup-init.sh -y --profile minimal
. "$HOME/.cargo/env"
```

The repository's `rust-toolchain.toml` selects Rust 1.95. `rustup` installs it
when you enter the checkout in the next section.

Compiler and package downloads are build-time network access. They are not
runtime calls made by the deployed endpoint. See the
[privacy boundary](PRIVACY.md#build-time-network-access-is-separate).

## Check out an exact source revision

Clone your reviewed repository and select a commit or reviewed tag. Do not build
an unspecified moving branch for production:

```bash
git clone <your-repository-url> TrustTunnel
cd TrustTunnel
git fetch --tags --prune
git checkout <commit-or-tag>
git status --short
git rev-parse HEAD
```

Now install the lint components for the repository-selected Rust 1.95
toolchain and verify the active tools:

```bash
rustup component add clippy rustfmt
rustup show active-toolchain
rustc --version --verbose
cargo --version
```

Create the Python environment outside the checkout. `tomli` is needed by the
compatibility tests on Python versions older than 3.11; pinning it also keeps
the command consistent on newer Ubuntu releases:

```bash
python3 -m venv ../trusttunnel-build-venv
. ../trusttunnel-build-venv/bin/activate
python -m pip install --disable-pip-version-check tomli==2.2.1
```

`git status --short` should be empty. Record the full commit printed by
`git rev-parse HEAD`; it identifies the source used for the package.

Enforce that condition immediately before building. Abort instead of labeling
an uncommitted tree with the current commit:

```bash
test -z "$(git status --porcelain --untracked-files=normal)" || {
    echo "Refusing to build from a dirty source tree" >&2
    exit 1
}
```

Review source and dependency changes before compiling. At minimum, inspect:

```bash
git show --stat --oneline HEAD
git diff <previous-reviewed-commit>..HEAD -- \
    Cargo.toml Cargo.lock endpoint lib tools scripts
```

The Git checkout stays source-only. Do not copy old binaries or libraries into
it to make a build pass.

## Build and verify the endpoint

Use the lock file so dependency resolution cannot silently select newer
versions:

```bash
cargo build --locked --release \
    --bin trusttunnel_endpoint --bin setup_wizard
```

Run all mandatory server checks on the capable VM:

```bash
make lint-rust
make test
```

If Markdown tooling is installed, also run:

```bash
make lint-md
```

Inspect the two resulting files:

```bash
file target/release/trusttunnel_endpoint
file target/release/setup_wizard
ldd target/release/trusttunnel_endpoint
ldd target/release/setup_wizard
```

`file` must report the VPS CPU architecture. `ldd` must not report any library
as `not found`. The endpoint's `--version` output is a software version, not a
fork or source-revision identifier. Use the package's `SOURCE_COMMIT` and binary
checksums for that purpose. Save the inspection output with internal build
records if you need an auditable release process.

## Create a package outside Git

From the repository root, stage only the expected runtime files:

```bash
distribution_dir="$(mktemp -d)"
install -d "$distribution_dir/trusttunnel"
install -m 0755 target/release/trusttunnel_endpoint \
    "$distribution_dir/trusttunnel/"
install -m 0755 target/release/setup_wizard \
    "$distribution_dir/trusttunnel/"
install -m 0644 scripts/trusttunnel.service.template \
    "$distribution_dir/trusttunnel/"
install -m 0644 LICENSE THIRD_PARTY_NOTICES.md \
    "$distribution_dir/trusttunnel/"
git rev-parse HEAD > "$distribution_dir/trusttunnel/SOURCE_COMMIT"
rustc --version --verbose > "$distribution_dir/trusttunnel/RUSTC_VERSION"
(
    . /etc/os-release
    printf 'ubuntu_version=%s\n' "$VERSION_ID"
    printf 'dpkg_architecture=%s\n' "$(dpkg --print-architecture)"
    uname -m
    ldd --version | head -n 1
) > "$distribution_dir/trusttunnel/TARGET_INFO"
```

Generate the dependency-license report from the same locked graph. Version
`0.9.1` is pinned here so the packaging tool itself does not drift between
builds:

```bash
cargo install --locked --version 0.9.1 --features cli cargo-about
cargo about generate --locked --offline --workspace --fail \
    --config scripts/licenses/about.toml \
    --output-file \
    "$distribution_dir/trusttunnel/THIRD_PARTY_LICENSES.html" \
    scripts/licenses/about.hbs
(
    cd "$distribution_dir/trusttunnel"
    sha256sum trusttunnel_endpoint setup_wizard > BINARY_SHA256SUMS
)
```

The command must succeed without a license warning. Do not weaken the accepted
license list merely to make packaging pass; review a new dependency and its
terms first. `cargo-about` is a build-time tool and is not included in the
server package.

Review the package manifest before archiving:

```bash
find "$distribution_dir/trusttunnel" -maxdepth 1 -type f \
    -printf '%f\n' | sort
```

It should list exactly:

```text
BINARY_SHA256SUMS
LICENSE
RUSTC_VERSION
SOURCE_COMMIT
TARGET_INFO
THIRD_PARTY_LICENSES.html
THIRD_PARTY_NOTICES.md
setup_wizard
trusttunnel.service.template
trusttunnel_endpoint
```

Create the archive in a sibling `dist` directory, not inside the repository:

```bash
install -d ../dist
ubuntu_release="$(. /etc/os-release; printf '%s' "$VERSION_ID" | tr . -)"
architecture="$(dpkg --print-architecture)"
artifact="trusttunnel-server-ubuntu-${ubuntu_release}-${architecture}.tar.gz"
tar --owner=0 --group=0 --numeric-owner \
    -C "$distribution_dir" -czf \
    "../dist/$artifact" trusttunnel
cd ../dist
sha256sum "$artifact" > "$artifact.sha256"
sha256sum -c "$artifact.sha256"
printf 'Release asset: %s\n' "$artifact"
```

Keep the printed filename. The commands below use
`trusttunnel-server-ubuntu-22-04-amd64.tar.gz` as an example; substitute the
exact filename printed on your VM.

Do not run `git add -f` on the archive. A release asset or `scp` transfer is not
a reason to commit compiled output.

## Transfer option A: `scp`

`scp` is the simplest choice when the VM can reach the VPS over SSH. From the
VM's `dist` directory:

```bash
artifact=trusttunnel-server-ubuntu-22-04-amd64.tar.gz
ssh_target=<ssh-user>@<vps-address>
transfer_dir=/tmp/trusttunnel-server-build-YYYYMMDDTHHMMSSZ
ssh "$ssh_target" \
    "test ! -e '$transfer_dir' && install -d -m 0700 '$transfer_dir'"
scp "$artifact" "$artifact.sha256" "$ssh_target:$transfer_dir/"
```

SSH to the VPS and verify the archive before using it:

```bash
ssh <ssh-user>@<vps-address>
(
set -eu
cd /tmp/trusttunnel-server-build-YYYYMMDDTHHMMSSZ
artifact=trusttunnel-server-ubuntu-22-04-amd64.tar.gz
sha256sum -c "$artifact.sha256"
)
```

The printed result must be `OK`. If it is not, stop and transfer again; do not
install a partially transferred package.

## Transfer option B: GitHub Release assets

GitHub Releases can store binary assets associated with a Git tag. Assets are
not part of the Git tree, so this keeps the repository itself source-only. A
public release makes the binary downloadable by anyone. A private release
requires repository access. Neither is an appropriate place for live secrets.
The checksum detects transfer corruption, but an attacker who can replace both
assets can also replace it; independently verify the tag and compare the
package's full `SOURCE_COMMIT` with the commit reviewed on the build VM.

Install and authenticate the official GitHub CLI on the build VM according to
your GitHub account policy. Create and push a tag that points to the exact
source commit:

```bash
cd /path/to/TrustTunnel
test -z "$(git status --porcelain --untracked-files=normal)" || {
    echo "Refusing to tag a dirty source tree" >&2
    exit 1
}
tag="server-build-$(date -u +%Y%m%dT%H%M%SZ)"
repository=alexeyshmelev/TTDO
git tag -a "$tag" -m "Build TrustTunnel server"
git push "git@github.com:${repository}.git" "$tag"
printf 'Release tag: %s\n' "$tag"
```

The repository workflow validates source but does not create release assets.
Create a draft Release manually and attach both files from the sibling `dist`
directory. Replace `YYYYMMDDTHHMMSSZ` with the timestamp in the printed tag:

```bash
artifact=trusttunnel-server-ubuntu-22-04-amd64.tar.gz
tag=server-build-YYYYMMDDTHHMMSSZ
repository=alexeyshmelev/TTDO
gh release create "$tag" \
    "../dist/$artifact" \
    "../dist/$artifact.sha256" \
    --draft \
    --repo "$repository" \
    --verify-tag \
    --title "TrustTunnel server build YYYY-MM-DD" \
    --notes "Built from source commit $(git rev-parse HEAD); no configuration or secrets."
```

Inspect the draft and verify that exactly the archive and its checksum are
attached—no TOML, certificate, key, client configuration, shell history, or
log. Then publish it:

```bash
tag=server-build-YYYYMMDDTHHMMSSZ
gh release view "$tag" \
    --json tagName,isDraft,assets,url
gh release edit "$tag" --draft=false
```

For a public repository, the VPS can download only the two named assets over
HTTPS without installing or authenticating the GitHub CLI:

```bash
(
set -eu
tag=server-build-YYYYMMDDTHHMMSSZ
download_dir="/tmp/trusttunnel-download-$tag"
test ! -e "$download_dir"
install -d -m 0700 "$download_dir"
cd "$download_dir"
sudo apt-get update
sudo apt-get install --no-install-recommends ca-certificates curl
artifact=trusttunnel-server-ubuntu-22-04-amd64.tar.gz
release_base="https://github.com/alexeyshmelev/TTDO/releases/download/$tag"
curl --fail --location --remote-name "$release_base/$artifact"
curl --fail --location --remote-name "$release_base/$artifact.sha256"
sha256sum -c "$artifact.sha256"
printf 'Download directory: %s\n' "$download_dir"
)
```

For a private repository, avoid leaving a broad GitHub token on the VPS. A
safer pattern is to download the assets on your authenticated PC or build VM,
verify them there, and use `scp` for the final hop.

Official references:

- [GitHub releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)
- [`gh release create`](https://cli.github.com/manual/gh_release_create)
- [`gh release download`](https://cli.github.com/manual/gh_release_download)

## Preflight the package on the VPS

Install the administration tools used below on the VPS once. `ufw` is needed
only when you follow this guide's UFW firewall steps:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends \
    ca-certificates curl file openssl ufw
```

Run this from the VPS directory containing the verified archive, before
stopping or disabling an old installation. Use the exact asset filename from
your VM:

```bash
artifact=trusttunnel-server-ubuntu-22-04-amd64.tar.gz
preflight_dir="$(mktemp -d)"
tar --no-same-owner -xzf "$artifact" -C "$preflight_dir"
cat "$preflight_dir/trusttunnel/TARGET_INFO"
file "$preflight_dir/trusttunnel/trusttunnel_endpoint"
file "$preflight_dir/trusttunnel/setup_wizard"
ldd "$preflight_dir/trusttunnel/trusttunnel_endpoint"
ldd "$preflight_dir/trusttunnel/setup_wizard"
"$preflight_dir/trusttunnel/trusttunnel_endpoint" --version
"$preflight_dir/trusttunnel/setup_wizard" --help >/dev/null
```

Both `file` results must match the VPS architecture, and neither `ldd` result
may contain `not found` or require a newer glibc. The endpoint must print its
version and the wizard help command must exit successfully. Stop here if any
check fails; rebuild on a compatible VM instead of disabling the working
package.

## Migrate an existing packaged installation

Skip this section on a VPS that has never run TrustTunnel. Do not unpack a
source build over an existing packaged installation: first preserve the exact
unit, executable, configuration, certificate, and key paths used by that
installation.

Find the installed unit name, then inspect it. `trusttunnel` below is an
example; replace it if the package uses another name:

```bash
systemctl list-unit-files --type=service | grep -i trusttunnel
old_unit=trusttunnel
sudo systemctl status "$old_unit" --no-pager
sudo systemctl show "$old_unit" \
    -p FragmentPath -p ExecStart -p User -p Group
sudo systemctl cat "$old_unit"
sudo ss -lntup '( sport = :443 )'
```

The `FragmentPath`, `ExecStart`, and unit contents identify the files that must
be recoverable. Create a root-only backup directory and save the unit metadata:

```bash
old_unit=trusttunnel
backup_dir="/root/trusttunnel-pre-source-$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -o root -g root -m 0700 "$backup_dir"
sudo systemctl show "$old_unit" \
    -p FragmentPath -p ExecStart -p User -p Group |
    sudo tee "$backup_dir/unit-properties.txt" >/dev/null
sudo systemctl cat "$old_unit" |
    sudo tee "$backup_dir/unit-effective.txt" >/dev/null
printf 'Backup directory: %s\n' "$backup_dir"
```

Keep that printed path. From the inspected command and unit, copy every
configuration directory, credential file, certificate, private key, and
executable outside `/opt/trusttunnel` into the backup with `sudo cp -a`. For
example, copy `/etc/trusttunnel` if that is the path the old unit actually
uses. Do not guess paths, and do not upload this backup because it contains
live secrets.

Stop and disable the old unit, then verify that its listener is gone:

```bash
old_unit=trusttunnel
sudo systemctl disable --now "$old_unit"
sudo ss -lntup '( sport = :443 )'
```

If anything still listens on TCP or UDP port 443, identify its owning process
and do not continue until the conflict is understood. Do not kill an unrelated
listener merely to make the port free.

Preserve a same-path installation and any local unit override before creating
the source-built installation:

```bash
(
set -eu
old_unit=trusttunnel
old_unit="${old_unit%.service}"
backup_dir=/root/trusttunnel-pre-source-REPLACE_WITH_RECORDED_TIMESTAMP
if ! sudo test -d "$backup_dir"; then
    echo "Backup directory not found: $backup_dir" >&2
    exit 1
fi
if sudo test -e /opt/trusttunnel; then
    sudo mv /opt/trusttunnel "$backup_dir/opt-trusttunnel"
fi
if [ "$old_unit" = trusttunnel ]; then
    if sudo test -e /etc/systemd/system/trusttunnel.service; then
        sudo mv /etc/systemd/system/trusttunnel.service "$backup_dir/"
    fi
    if sudo test -e /etc/systemd/system/trusttunnel.service.d; then
        sudo mv /etc/systemd/system/trusttunnel.service.d "$backup_dir/"
    fi
    sudo systemctl daemon-reload
fi
)
```

After extracting the source-built package in the next section, choose whether
to [reuse compatible migration files](#choose-configuration) or create new
configuration with the wizard. Reusing the reviewed credentials and
certificate lets existing profiles continue working; generating either value
again requires a new profile on every client.

Leave the old distribution package installed until the source build has passed
a real client connection test. Generate or carefully review new configuration;
do not assume that an old package's paths and settings are compatible. If you
run the wizard and generate a new credential or certificate instead of reusing
the old values, export and securely re-import a new profile on every client.

### Roll back the package migration

If the source-built service fails, reuse the old unit name and the exact backup
directory printed above. Replace the timestamp placeholder before running this
block; its directory check deliberately stops before changing anything if the
path is wrong:

```bash
(
set -eu
old_unit=trusttunnel
backup_dir=/root/trusttunnel-pre-source-REPLACE_WITH_RECORDED_TIMESTAMP
if ! sudo test -d "$backup_dir"; then
    echo "Backup directory not found: $backup_dir" >&2
    exit 1
fi
failed_dir="$backup_dir/source-build-failed"
if sudo test -e "$failed_dir"; then
    echo "Preserve or remove the existing $failed_dir first" >&2
    exit 1
fi

sudo systemctl disable --now trusttunnel 2>/dev/null || true
sudo install -d -o root -g root -m 0700 "$failed_dir"
if sudo test -e /etc/systemd/system/trusttunnel.service; then
    sudo mv /etc/systemd/system/trusttunnel.service \
        "$failed_dir/"
fi
if sudo test -e /etc/systemd/system/trusttunnel.service.d; then
    sudo mv /etc/systemd/system/trusttunnel.service.d \
        "$failed_dir/"
fi
if sudo test -e /opt/trusttunnel; then
    sudo mv /opt/trusttunnel "$failed_dir/opt-trusttunnel"
fi
if sudo test -e "$backup_dir/trusttunnel.service"; then
    sudo mv "$backup_dir/trusttunnel.service" /etc/systemd/system/
fi
if sudo test -e "$backup_dir/trusttunnel.service.d"; then
    sudo mv "$backup_dir/trusttunnel.service.d" /etc/systemd/system/
fi
if sudo test -e "$backup_dir/opt-trusttunnel"; then
    sudo mv "$backup_dir/opt-trusttunnel" /opt/trusttunnel
fi
sudo systemctl daemon-reload
sudo systemctl enable --now "$old_unit"
sudo systemctl status "$old_unit" --no-pager
sudo ss -lntup '( sport = :443 )'
)
```

Restore any separately backed-up external configuration to the exact paths
recorded from the old unit, if those paths were changed. Confirm that only the
expected process owns port 443 and that an old client can connect before
removing either installation backup.

## Install and run on the VPS

After checksum verification, create a dedicated service account and directory:

```bash
(
set -eu
if ! getent group trusttunnel >/dev/null 2>&1; then
    sudo groupadd --system trusttunnel
fi
if ! id -u trusttunnel >/dev/null 2>&1; then
    sudo useradd --system --home-dir /opt/trusttunnel \
        --gid trusttunnel --shell /usr/sbin/nologin trusttunnel
fi
getent group trusttunnel
getent passwd trusttunnel
sudo install -d -o root -g trusttunnel -m 0750 /opt/trusttunnel
artifact=trusttunnel-server-ubuntu-22-04-amd64.tar.gz
sudo tar --no-same-owner -xzf "$artifact" \
    -C /opt/trusttunnel --strip-components=1
sudo chown root:trusttunnel /opt/trusttunnel
sudo chmod 0750 /opt/trusttunnel
sudo chown root:root \
    /opt/trusttunnel/trusttunnel_endpoint \
    /opt/trusttunnel/setup_wizard
sudo chmod 0755 \
    /opt/trusttunnel/trusttunnel_endpoint \
    /opt/trusttunnel/setup_wizard
sudo sh -c '
set -eu
cd /opt/trusttunnel
sha256sum -c BINARY_SHA256SUMS
printf "Source commit: "
cat SOURCE_COMMIT
cat TARGET_INFO
'
)
```

If the service account already exists, confirm that the two `getent` lines
show the expected dedicated account and group. Do not delete or recreate an
existing account without checking its ownership first.

### Choose configuration

For a package migration, you may reuse compatible configuration so existing
clients keep the same credentials and certificate. First review the old files
against the current configuration reference. In particular, make sure every
relative certificate path in `hosts.toml` still resolves below
`/opt/trusttunnel`.

If the old installation used the standard filenames below, copy only those
runtime files from the recorded backup. Replace the timestamp first; the
subshell refuses to overwrite files already created in the new installation:

```bash
(
set -eu
backup_dir=/root/trusttunnel-pre-source-REPLACE_WITH_RECORDED_TIMESTAMP
if ! sudo test -d "$backup_dir/opt-trusttunnel"; then
    echo "Old installation backup not found: $backup_dir" >&2
    exit 1
fi
for required in vpn.toml hosts.toml credentials.toml; do
    if ! sudo test -f "$backup_dir/opt-trusttunnel/$required"; then
        echo "Required backup file not found: $required" >&2
        exit 1
    fi
done
for item in vpn.toml hosts.toml credentials.toml rules.toml certs; do
    source_path="$backup_dir/opt-trusttunnel/$item"
    target_path="/opt/trusttunnel/$item"
    if sudo test -e "$source_path" && sudo test -e "$target_path"; then
        echo "Refusing to overwrite $target_path" >&2
        exit 1
    fi
done
for item in vpn.toml hosts.toml credentials.toml rules.toml certs; do
    source_path="$backup_dir/opt-trusttunnel/$item"
    if sudo test -e "$source_path"; then
        sudo cp -a "$source_path" /opt/trusttunnel/
    fi
done
)
```

If the old unit used different or external paths, adapt this procedure to the
exact files recorded from that unit. Do not move Let's Encrypt or another
certificate store without also updating and reviewing `hosts.toml`.

For a fresh configuration, run the wizard as an administrator because binding
ACME port 80 and writing protected certificate files may require it. Skip this
step when you deliberately restored and reviewed compatible migration files:

If you select ACME HTTP-01, first replace the hostname below and verify that its
public address points to this VPS and that no other process owns TCP port 80:

```bash
getent ahosts vpn.example.com
sudo ss -lntp '( sport = :80 )'
sudo ufw status
```

If UFW does not already list an allow rule for `80/tcp`, add one and remember
that this procedure created it. Add the same temporary rule in the VPS
provider's firewall:

```bash
sudo ufw allow 80/tcp
```

```bash
sudo sh -c 'cd /opt/trusttunnel && exec ./setup_wizard'
```

After successful issuance, run the removal command below only if this procedure
added the rule. Remove the matching provider-firewall rule on the same
condition. Leave pre-existing web-server or documented renewal rules intact:

```bash
sudo ufw delete allow 80/tcp
```

If the wizard generated a new credential or certificate instead of reusing the
old values, export and securely re-import a new profile on every client.

Whether you restored files or ran the wizard, keep relative paths inside
`/opt/trusttunnel`, then grant the service read access without making secrets
world-readable:

```bash
sudo sh -c '
set -eu
cd /opt/trusttunnel
for file in vpn.toml hosts.toml credentials.toml rules.toml; do
    if [ -f "$file" ]; then
        chown root:trusttunnel "$file"
        chmod 0640 "$file"
    fi
done
if [ -d certs ]; then
    find certs -type d -exec chown root:trusttunnel {} +
    find certs -type d -exec chmod 0750 {} +
    find certs -type f -exec chown root:trusttunnel {} +
    find certs -type f -exec chmod 0640 {} +
fi
'
```

If an optional file or directory was not created, the loop leaves it absent
rather than creating an empty substitute.

For the simplest 512 MB baseline, keep HTTP/2 and disable the HTTP/1.1 and
QUIC listeners before first start. Back up the generated file, then edit it:

```bash
sudo cp -a /opt/trusttunnel/vpn.toml \
    /opt/trusttunnel/vpn.toml.before-low-memory
sudoedit /opt/trusttunnel/vpn.toml
```

Delete the complete `[listen_protocols.http1]` table, from that header through
the line before `[listen_protocols.http2]`. Leave the complete HTTP/2 table.
Delete the complete `[listen_protocols.quic]` table, from its header through
its final `message_queue_capacity` setting. Then verify that HTTP/2 is the only
active listener table:

```bash
grep -n '^\[listen_protocols\.' /opt/trusttunnel/vpn.toml
```

The command must print only `[listen_protocols.http2]`. Leave UDP port 443
closed until you intentionally restore and test QUIC.

Install the service template:

```bash
sudo install -m 0644 /opt/trusttunnel/trusttunnel.service.template \
    /etc/systemd/system/trusttunnel.service
sudo install -d -m 0755 /etc/systemd/system/trusttunnel.service.d
sudoedit /etc/systemd/system/trusttunnel.service.d/security.conf
```

Put this in `security.conf` for TCP and UDP forwarding on port 443:

```ini
[Service]
User=trusttunnel
Group=trusttunnel
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
LimitNOFILE=65536
```

If `[icmp]` is enabled deliberately, add `CAP_NET_RAW` to both capability
lines. Do not add it for ordinary TCP and UDP operation.

For the 512 MB baseline, set one Tokio worker before the first start. Create a
separate low-memory drop-in. You may omit this file on a larger VPS after
measuring its memory use:

```bash
sudoedit /etc/systemd/system/trusttunnel.service.d/low-memory.conf
```

Put these lines in `low-memory.conf`:

```ini
[Service]
ExecStart=
ExecStart=/opt/trusttunnel/trusttunnel_endpoint --jobs 1 vpn.toml hosts.toml
```

The empty `ExecStart=` resets the template's command before the replacement is
added. `security.conf` continues to hold the service account and capability
policy. Save the new drop-in, then load, start, and verify the unit:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now trusttunnel
sudo systemctl show trusttunnel -p ExecStart
sudo systemctl status trusttunnel --no-pager
sudo journalctl -u trusttunnel -b -n 100 --no-pager
sudo ss -lntup '( sport = :443 )'
```

## Firewall checklist

Before enabling UFW, inspect the SSH daemon's effective ports and allow SSH:

```bash
sudo sshd -T | awk '$1 == "port" {print $2}'
sudo ufw allow OpenSSH
```

The `OpenSSH` profile normally covers only TCP port 22. If the command prints a
custom port, allow every actual SSH port; replace 2222 with the real value:

```bash
sudo ufw allow 2222/tcp
```

Then add the HTTP/2 baseline listener and enable UFW:

```bash
sudo ufw allow 443/tcp
sudo ufw enable
sudo ufw status verbose
```

Only after intentionally enabling and testing QUIC, add its UDP rule:

```bash
sudo ufw allow 443/udp
sudo ufw status verbose
```

- Keep the current SSH session open and verify a second SSH login before
  closing it.
- Keep `443/udp` closed while HTTP/3 is disabled.
- Allow `80/tcp` only for ACME HTTP-01 issuance or renewal.
- Add the actual SSH port and the same TrustTunnel rules to the VPS provider
  firewall.
- Allow outbound traffic to destinations clients should reach.
- Do not enable kernel IP forwarding, a server TUN, or NAT for the native
  direct forwarder.

## Validate the deployment

Confirm that DNS points to the VPS and that a TLS client sees the expected
certificate:

```bash
getent ahosts vpn.example.com
openssl s_client -connect vpn.example.com:443 \
    -servername vpn.example.com -verify_hostname vpn.example.com \
    -verify_return_error -alpn h2 </dev/null
```

The output should include `Verify return code: 0 (ok)` and `ALPN protocol: h2`
when HTTP/2 is configured.

Export a new private client TOML and connect one test client. Verify ordinary
TCP browsing and a UDP-based operation. Enable and test HTTP/3 only after the
HTTP/2 baseline works.

Monitor initial memory use:

```bash
systemctl show trusttunnel -p MemoryCurrent -p TasksCurrent
free -h
sudo journalctl -k -b --no-pager | grep -i -E 'oom|killed process'
```

If runtime memory is tight, keep one worker, disable unused transports and
optional services, and leave ICMP off. Do not attempt a release build on the
small VPS.

## Update and roll back

Build every update from a new reviewed commit and repeat tests, package review,
checksum generation, and transfer. Never put live configuration into the new
archive.

Create a new root-only staging directory before copying or downloading the
update. Refuse to reuse an old directory because it may mix package versions:

```bash
(
set -eu
update_dir=/tmp/trusttunnel-update
if [ -e "$update_dir" ]; then
    echo "Preserve or remove the existing $update_dir first" >&2
    exit 1
fi
install -d -m 0700 "$update_dir"
)
```

Copy the archive and checksum into that directory. Then verify the archive,
binary manifest, recorded source commit, architecture, dynamic libraries, and
executability before staging anything under `/opt`. Compare the full commit ID
with the commit or release tag reviewed on the build VM:

```bash
(
set -eu
cd /tmp/trusttunnel-update
artifact=trusttunnel-server-ubuntu-22-04-amd64.tar.gz
sha256sum -c "$artifact.sha256"
test ! -e trusttunnel
tar --no-same-owner -xzf "$artifact"
cd trusttunnel
sha256sum -c BINARY_SHA256SUMS
printf 'Source commit: '
cat SOURCE_COMMIT
cat TARGET_INFO
file trusttunnel_endpoint setup_wizard
ldd trusttunnel_endpoint
ldd setup_wizard
./trusttunnel_endpoint --version
./setup_wizard --help >/dev/null
)
```

The endpoint's `--version` output may remain `1.0.41` across fork-specific
changes, so it cannot replace this provenance check. Endpoint `v*` tags are
independent of native client `client-v*` tags. Stage the binaries and matching
metadata without replacing the running files:

```bash
(
set -eu
cd /tmp/trusttunnel-update
sudo install -o root -g root -m 0755 \
    trusttunnel/trusttunnel_endpoint \
    /opt/trusttunnel/trusttunnel_endpoint.new
sudo install -o root -g root -m 0755 \
    trusttunnel/setup_wizard \
    /opt/trusttunnel/setup_wizard.new
for file in BINARY_SHA256SUMS LICENSE RUSTC_VERSION SOURCE_COMMIT TARGET_INFO \
    THIRD_PARTY_LICENSES.html THIRD_PARTY_NOTICES.md \
    trusttunnel.service.template; do
    sudo install -o root -g root -m 0644 "trusttunnel/$file" \
        "/opt/trusttunnel/$file.new"
done
)
```

Stop briefly, preserve the known-good files, switch them, and start. The root
subshell refuses to reuse an older rollback directory:

```bash
(
set -eu
sudo sh -c '
set -eu
cd /opt/trusttunnel
rollback=rollback-previous
if [ -e "$rollback" ]; then
    echo "Preserve or remove the existing $rollback directory first" >&2
    exit 1
fi
for file in trusttunnel_endpoint setup_wizard BINARY_SHA256SUMS LICENSE \
    RUSTC_VERSION SOURCE_COMMIT TARGET_INFO THIRD_PARTY_LICENSES.html \
    THIRD_PARTY_NOTICES.md trusttunnel.service.template; do
    test -f "$file.new" || {
        echo "Missing staged file: $file.new" >&2
        exit 1
    }
done
'
sudo systemctl stop trusttunnel
sudo sh -c '
set -eu
cd /opt/trusttunnel
rollback=rollback-previous
install -d -o root -g root -m 0700 "$rollback"
mv trusttunnel_endpoint setup_wizard "$rollback/"
for file in BINARY_SHA256SUMS LICENSE RUSTC_VERSION SOURCE_COMMIT TARGET_INFO \
    THIRD_PARTY_LICENSES.html THIRD_PARTY_NOTICES.md \
    trusttunnel.service.template; do
    if [ -e "$file" ]; then
        mv "$file" "$rollback/"
    fi
done
mv trusttunnel_endpoint.new trusttunnel_endpoint
mv setup_wizard.new setup_wizard
for file in BINARY_SHA256SUMS LICENSE RUSTC_VERSION SOURCE_COMMIT TARGET_INFO \
    THIRD_PARTY_LICENSES.html THIRD_PARTY_NOTICES.md \
    trusttunnel.service.template; do
    mv "$file.new" "$file"
done
'
sudo systemctl start trusttunnel
sudo systemctl status trusttunnel --no-pager
sudo sh -c '
set -eu
cd /opt/trusttunnel
sha256sum -c BINARY_SHA256SUMS
printf "Source commit: "
cat SOURCE_COMMIT
cat TARGET_INFO
'
)
```

Each rename is atomic on one filesystem, but the versioned package set is
switched sequentially while the service is stopped; it is not an atomic group
replacement. Test a client before deleting the rollback directory.

The installed systemd unit does not change merely because the packaged template
changed. Diff and review the old and new templates first:

```bash
sudo diff -u \
    /opt/trusttunnel/rollback-previous/trusttunnel.service.template \
    /opt/trusttunnel/trusttunnel.service.template || true
```

If the diff is intentional, preserve the active unit, install the reviewed
template, reload systemd, and restart. Keep the drop-in under
`trusttunnel.service.d` because it contains the local service account and
capability policy:

```bash
(
set -eu
unit_backup=/opt/trusttunnel/rollback-previous/trusttunnel.service.active-previous
unit_marker=/opt/trusttunnel/rollback-previous/unit-was-updated
if sudo test -e "$unit_backup" || sudo test -e "$unit_marker"; then
    echo "Preserve or remove the existing unit rollback files first" >&2
    exit 1
fi
sudo cp -a /etc/systemd/system/trusttunnel.service \
    "$unit_backup"
sudo install -o root -g root -m 0600 /dev/null "$unit_marker"
sudo install -o root -g root -m 0644 \
    /opt/trusttunnel/trusttunnel.service.template \
    /etc/systemd/system/trusttunnel.service
sudo systemctl daemon-reload
sudo systemctl restart trusttunnel
sudo systemctl status trusttunnel --no-pager
)
```

If no template change is needed, leave the active unit untouched. If startup,
the client test, or a reviewed unit change fails, preserve the failed set and
restore the previous files:

```bash
(
set -eu
sudo sh -c '
set -eu
cd /opt/trusttunnel
test -d rollback-previous
test -x rollback-previous/trusttunnel_endpoint
test -x rollback-previous/setup_wizard
if [ -e rollback-previous/unit-was-updated ]; then
    test -f rollback-previous/trusttunnel.service.active-previous
fi
if [ -e failed-update ]; then
    echo "Preserve or remove the existing failed-update directory first" >&2
    exit 1
fi
'
sudo systemctl stop trusttunnel
sudo sh -c '
set -eu
cd /opt/trusttunnel
rollback=rollback-previous
failed=failed-update
install -d -o root -g root -m 0700 "$failed"
mv trusttunnel_endpoint setup_wizard "$failed/"
mv "$rollback/trusttunnel_endpoint" trusttunnel_endpoint
mv "$rollback/setup_wizard" setup_wizard
for file in BINARY_SHA256SUMS LICENSE RUSTC_VERSION SOURCE_COMMIT TARGET_INFO \
    THIRD_PARTY_LICENSES.html THIRD_PARTY_NOTICES.md \
    trusttunnel.service.template; do
    if [ -e "$file" ]; then
        mv "$file" "$failed/"
    fi
    if [ -e "$rollback/$file" ]; then
        mv "$rollback/$file" "$file"
    fi
done
'
sudo sh -c '
set -eu
cd /opt/trusttunnel
unit_backup=rollback-previous/trusttunnel.service.active-previous
if [ -e "$unit_backup" ]; then
    install -o root -g root -m 0644 "$unit_backup" \
        /etc/systemd/system/trusttunnel.service
    systemctl daemon-reload
fi
'
sudo systemctl start trusttunnel
sudo systemctl status trusttunnel --no-pager
sudo sh -c '
set -eu
cd /opt/trusttunnel
if [ -e BINARY_SHA256SUMS ]; then
    sha256sum -c BINARY_SHA256SUMS
fi
printf "Source commit: "
cat SOURCE_COMMIT
cat TARGET_INFO
'
)
```

This procedure does not touch `vpn.toml`, `hosts.toml`, credentials, rules, or
certificates. If an update introduces a configuration migration, back up those
files separately and document the reverse migration before changing them.

## Renew certificates and reload

Replace certificate and key files atomically and keep ownership readable by the
service group. Then ask only the main endpoint process to reload its TLS host
configuration:

```bash
sudo systemctl kill --kill-who=main --signal=HUP trusttunnel
sudo journalctl -u trusttunnel -n 20 --no-pager
```

`SIGHUP` reloads `hosts.toml` and referenced certificate material. Restart the
service for changes to `vpn.toml`, credentials, rules, worker count, or systemd
settings.

## Keep build and runtime data separate

Use this final boundary as a release checklist:

```text
Git repository                 Release asset                 VPS state
--------------                 -------------                 ---------
source                          endpoint executable           vpn.toml
manifests and lock files        setup wizard executable       hosts.toml
tests and documentation         service template              credentials.toml
licenses                        source commit metadata         rules.toml
                                                               certificates
no compiled output              no secrets                     never published
```

The [official Rust installation guide](https://www.rust-lang.org/learn/get-started)
describes `rustup`. Platform client builds have separate prerequisites and
signing boundaries in [the client documentation](../clients/README.md).
