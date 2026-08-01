#!/bin/bash

set -e

check_file() {
    local file="$1"
    if [ ! -f "$file" ]; then
        echo "Configuration file '$file' not found"
        return 1
    fi
    return 0
}

verify_configs() {
    local missing=0
    local present=0
    local residual=0

    for file in credentials.toml vpn.toml hosts.toml; do
        if check_file "$file"; then
            present=$((present + 1))
        else
            missing=$((missing + 1))
        fi
    done

    for file in rules.toml certs/cert.pem certs/key.pem; do
        if [ -e "$file" ] || [ -L "$file" ]; then
            residual=1
        fi
    done

    if [ "$missing" -eq 0 ]; then
        return 0
    fi
    if [ "$present" -eq 0 ] && [ "$residual" -eq 0 ]; then
        return 1
    fi

    echo "Error: Partial configuration detected. Keep all existing files and restore the missing file(s); automatic setup only runs when all configuration files are absent"
    return 2
}

run_setup_wizard_noninteractive() {
    local credentials_file="${TT_CREDENTIALS_FILE:-/run/secrets/trusttunnel_credentials}"

    if [ -z "${TT_HOSTNAME:-}" ]; then
        echo "Error: TT_HOSTNAME is required for non-interactive setup"
        return 1
    fi
    if [ ! -e "$credentials_file" ]; then
        echo "Error: Credentials file '$credentials_file' not found. Set TT_CREDENTIALS_FILE or mount it at /run/secrets/trusttunnel_credentials"
        return 1
    fi

    local args=(
        "-m" "non-interactive"
        "-a" "${TT_LISTEN_ADDRESS:-0.0.0.0:8443}"
        "--creds-file" "$credentials_file"
        "-n" "$TT_HOSTNAME"
        "--lib-settings" "vpn.toml"
        "--hosts-settings" "hosts.toml"
    )

    case "${TT_CERT_TYPE:-self-signed}" in
        self-signed)
            args+=("--cert-type" "self-signed")
            ;;
        letsencrypt)
            if [ -z "${TT_ACME_EMAIL:-}" ]; then
                echo "Error: TT_ACME_EMAIL is required when TT_CERT_TYPE=letsencrypt"
                return 1
            fi
            args+=("--cert-type" "letsencrypt" "--acme-email" "$TT_ACME_EMAIL")
            if [ "${TT_ACME_STAGING:-false}" = "true" ]; then
                args+=("--acme-staging")
            fi
            ;;
        provided)
            if [ -z "${TT_CERT_PROVIDED_CHAIN_PATH:-}" ] || [ -z "${TT_CERT_PROVIDED_KEY_PATH:-}" ]; then
                echo "Error: TT_CERT_PROVIDED_CHAIN_PATH and TT_CERT_PROVIDED_KEY_PATH are required when TT_CERT_TYPE=provided"
                return 1
            fi
            args+=(
                "--cert-type" "provided"
                "--cert-chain-path" "$TT_CERT_PROVIDED_CHAIN_PATH"
                "--cert-key-path" "$TT_CERT_PROVIDED_KEY_PATH"
            )
            ;;
        *)
            echo "Error: Unsupported TT_CERT_TYPE='$TT_CERT_TYPE'. Supported: self-signed, letsencrypt, provided"
            return 1
            ;;
    esac

    echo "Missing configuration file(s). Running setup_wizard in non-interactive mode"
    setup_wizard "${args[@]}"
}

main() {
    local config_state=0
    verify_configs || config_state=$?

    case "$config_state" in
        0)
            ;;
        1)
            if [ -t 0 ]; then
                echo "Missing configuration files. Launching setup wizard in interactive mode"
                setup_wizard
            else
                run_setup_wizard_noninteractive
            fi
            ;;
        *)
            return 1
            ;;
    esac

    echo "Starting trusttunnel_endpoint"
    exec trusttunnel_endpoint vpn.toml hosts.toml
}

main
