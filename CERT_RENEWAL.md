# Let's Encrypt certificate renewal

## Table of contents

- [Prerequisites](#prerequisites)
- [Install Certbot](#install-certbot)
- [Issue the certificate](#issue-the-certificate)
    - [Option A: standalone](#option-a-standalone)
    - [Option B: webroot](#option-b-webroot)
- [Find the Certbot certificate name](#find-the-certbot-certificate-name)
- [Install the deployment hook](#install-the-deployment-hook)
- [Deploy the certificate initially](#deploy-the-certificate-initially)
- [Configure TrustTunnel](#configure-trusttunnel)
- [Enable automatic renewal](#enable-automatic-renewal)
- [Test renewal and reload](#test-renewal-and-reload)
- [Roll back a certificate deployment](#roll-back-a-certificate-deployment)
- [Troubleshooting](#troubleshooting)

TrustTunnel needs a valid TLS certificate. This guide uses [Certbot][certbot]
to obtain and renew a Let's Encrypt certificate without allowing the
unprivileged TrustTunnel service to read `/etc/letsencrypt`.

The root-run deployment hook copies each renewed certificate and private key
into a new, restricted directory under `/opt/trusttunnel/certs`. It then
atomically switches the `current` symlink and sends `SIGHUP` to the main
TrustTunnel process. A reload therefore sees either the complete old pair or
the complete new pair, never one file from each deployment.

[certbot]: https://eff-certbot.readthedocs.io/en/stable/

## Prerequisites

- A public DNS name whose A or AAAA record points to the endpoint.
- Port **80/tcp** reachable from the Internet during HTTP-01 validation.
- Root access on the endpoint.
- TrustTunnel installed as the `trusttunnel` systemd service, running with the
  `trusttunnel` group.

This guide uses HTTP-01. Use a Certbot DNS plugin instead if you need a
wildcard certificate. If your service or group has a different name, replace
both values in the hook before installing it.

## Install Certbot

Use the installation method recommended for your distribution. On
Debian/Ubuntu:

```bash
sudo apt update
sudo apt install -y certbot
```

## Issue the certificate

Choose one mode.

### Option A: standalone

Use standalone mode if nothing else listens on port 80. Certbot temporarily
starts its own HTTP server:

```bash
sudo certbot certonly --standalone -d example.com
```

### Option B: webroot

Use webroot mode if an existing HTTP server serves
`/.well-known/acme-challenge/` from the selected directory:

```bash
sudo certbot certonly --webroot -w /var/www/html -d example.com
```

Certbot stores the certificate lineage under `/etc/letsencrypt/live`. Do not
point the unprivileged TrustTunnel process at that root-owned tree.

## Find the Certbot certificate name

List the certificates managed by Certbot:

```bash
sudo certbot certificates
```

Find the `Certificate Name` for `example.com`. Use that exact name below. It
may be `example.com`, `example.com-0001`, or another suffixed name. The example
hook assumes this lineage:

```text
/etc/letsencrypt/live/example.com
```

## Install the deployment hook

Create Certbot's standard deploy-hook directory and open the hook in a root
editor:

```bash
sudo install -d -o root -g root -m 0755 \
    /etc/letsencrypt/renewal-hooks/deploy
sudoedit /etc/letsencrypt/renewal-hooks/deploy/trusttunnel
```

Paste the following script. Change `expected_lineage` if the exact certificate
name reported by `certbot certificates` is not `example.com`:

```sh
#!/bin/sh
set -eu

expected_lineage="/etc/letsencrypt/live/example.com"
cert_root="/opt/trusttunnel/certs"
service_name="trusttunnel"
lineage="${RENEWED_LINEAGE:-$expected_lineage}"

if [ "$(id -u)" -ne 0 ]; then
    echo "The TrustTunnel Certbot hook must run as root." >&2
    exit 1
fi

if [ "$lineage" != "$expected_lineage" ]; then
    exit 0
fi

if ! getent group trusttunnel >/dev/null; then
    echo "The trusttunnel group does not exist." >&2
    exit 1
fi

for filename in fullchain.pem privkey.pem; do
    if [ ! -r "$lineage/$filename" ]; then
        echo "Cannot read $lineage/$filename." >&2
        exit 1
    fi
done

install -d -o root -g trusttunnel -m 0750 "$cert_root"
release_dir="$(mktemp -d "$cert_root/.release.XXXXXX")"
chown root:trusttunnel "$release_dir"
chmod 0750 "$release_dir"

install -o root -g trusttunnel -m 0640 \
    "$lineage/fullchain.pem" "$release_dir/fullchain.pem"
install -o root -g trusttunnel -m 0640 \
    "$lineage/privkey.pem" "$release_dir/privkey.pem"

release_name="${release_dir##*/}"
next_link="$cert_root/.current-$release_name"
ln -s "$release_name" "$next_link"

if [ -L "$cert_root/current" ]; then
    previous_target="$(readlink "$cert_root/current")"
    previous_link="$cert_root/.previous-$release_name"
    ln -s "$previous_target" "$previous_link"
    mv -Tf "$previous_link" "$cert_root/previous"
fi

mv -Tf "$next_link" "$cert_root/current"

if systemctl is-active --quiet "$service_name"; then
    systemctl kill --kill-who=main --signal=HUP "$service_name"
fi
```

Make the saved hook root-owned and executable, then check its shell syntax:

```bash
sudo chown root:root /etc/letsencrypt/renewal-hooks/deploy/trusttunnel
sudo chmod 0755 /etc/letsencrypt/renewal-hooks/deploy/trusttunnel
sudo sh -n /etc/letsencrypt/renewal-hooks/deploy/trusttunnel
```

Certbot supplies `RENEWED_LINEAGE` during a real renewal. The exact-lineage
check prevents renewal of another certificate on the same server from
replacing TrustTunnel's certificate.

## Deploy the certificate initially

Run the hook once to create the first release and `current` symlink:

```bash
sudo /etc/letsencrypt/renewal-hooks/deploy/trusttunnel
sudo -u trusttunnel test -r \
    /opt/trusttunnel/certs/current/fullchain.pem
sudo -u trusttunnel test -r \
    /opt/trusttunnel/certs/current/privkey.pem
```

The files should be owned by `root:trusttunnel`, with certificate and key mode
`0640` and their containing directories mode `0750`:

```bash
sudo namei -l /opt/trusttunnel/certs/current/privkey.pem
```

## Configure TrustTunnel

If you have not generated the TrustTunnel configuration, choose **Provide path
to existing certificate** in `setup_wizard` and enter these two paths:

```text
/opt/trusttunnel/certs/current/fullchain.pem
/opt/trusttunnel/certs/current/privkey.pem
```

For an existing installation, update `hosts.toml`:

```toml
[[main_hosts]]
hostname = "example.com"
cert_chain_path = "/opt/trusttunnel/certs/current/fullchain.pem"
private_key_path = "/opt/trusttunnel/certs/current/privkey.pem"
```

Reload the TLS host settings after saving the file:

```bash
sudo systemctl kill --kill-who=main --signal=HUP trusttunnel
sudo journalctl -u trusttunnel -n 20 --no-pager
```

## Enable automatic renewal

On most modern Linux distributions, the Certbot package installs a systemd
timer. Confirm it exists:

```bash
systemctl list-timers | grep -E 'certbot|letsencrypt'
```

If the installation does not provide a timer, add renewal to root's crontab:

```bash
sudo crontab -e
```

```cron
0 3 * * * /usr/bin/certbot renew --quiet
```

Executable files in `/etc/letsencrypt/renewal-hooks/deploy` run only after a
successful renewal. No `certbot reconfigure` command is required.

## Test renewal and reload

Run a dry run and explicitly ask Certbot to exercise deploy hooks:

```bash
sudo certbot renew --dry-run --run-deploy-hooks
sudo journalctl -u trusttunnel -n 20 --no-pager
```

Check that the service remained active and the staged files are readable:

```bash
systemctl is-active trusttunnel
sudo -u trusttunnel test -r \
    /opt/trusttunnel/certs/current/fullchain.pem
sudo -u trusttunnel test -r \
    /opt/trusttunnel/certs/current/privkey.pem
```

If you use standalone mode, port 80 must be available during the dry run.

## Roll back a certificate deployment

After the second successful deployment, the hook retains the prior release in
the `previous` symlink. Inspect both targets before rolling back:

```bash
sudo readlink /opt/trusttunnel/certs/current
sudo readlink /opt/trusttunnel/certs/previous
```

Atomically point `current` at the previous release and reload the endpoint:

```bash
cert_root="/opt/trusttunnel/certs"
previous_target="$(sudo readlink "$cert_root/previous")"
rollback_link="$cert_root/.rollback-current.$$"
sudo ln -s "$previous_target" "$rollback_link"
sudo mv -Tf "$rollback_link" "$cert_root/current"
sudo systemctl kill --kill-who=main --signal=HUP trusttunnel
sudo journalctl -u trusttunnel -n 20 --no-pager
```

Rerun the deployment hook to stage the current Certbot certificate again.
Do not delete the old release until a client connection succeeds.

## Troubleshooting

- **Port 80 is busy**: stop the listener temporarily, or use webroot mode.
- **DNS or firewall failure**: verify that the hostname resolves to this VPS
  and that inbound 80/tcp is allowed.
- **Wrong certificate lineage**: compare `expected_lineage` with the exact
  `Certificate Name` from `sudo certbot certificates`.
- **Permission failure**: confirm the service uses group `trusttunnel`, then
  inspect every path component with `namei -l`.
- **Reload failure**: inspect `sudo journalctl -u trusttunnel -n 50 --no-pager`.
- **Failed interrupted hook**: a hidden `.release.*` directory may remain. Do
  not remove `current`, `previous`, or either target while the service uses it.
