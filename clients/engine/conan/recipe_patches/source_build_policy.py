def force_transitive_source_builds(provider_path):
    """Make a Conan provider force source builds without rewriting it twice."""
    with open(provider_path, "r", encoding="utf-8", newline="") as provider_file:
        provider = provider_file.read()

    if "--build=missing" in provider:
        provider = provider.replace("--build=missing", "--build=*")
        with open(provider_path, "w", encoding="utf-8", newline="") as provider_file:
            provider_file.write(provider)
    elif "--build=*" not in provider:
        raise RuntimeError("The Conan provider has no recognized build policy")
