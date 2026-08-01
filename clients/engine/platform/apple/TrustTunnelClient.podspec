# Resolve the local pod version: TT_CLIENT_VERSION env var -> git describe
# --tags --match client-v* -> 0.0.0-git fallback.
def resolve_tt_client_version
  env = ENV['TT_CLIENT_VERSION']
  return env.strip unless env.nil? || env.strip.empty?

  described = `git describe --tags --match 'client-v*' 2>/dev/null`.strip
  return described.sub(/^client-v/, '') unless described.empty?

  '0.0.0-git'
end

Pod::Spec.new do |s|
  s.name         = "TrustTunnelClient"
  s.module_name  = "TrustTunnelClient"
  s.version      = resolve_tt_client_version
  s.summary      = "TrustTunnelClient Apple adapter"
  s.description  = <<-DESC
                  Local TrustTunnelClient adapter for macOS and iOS
                   DESC
  s.homepage     = "https://github.com/alexeyshmelev/TTDO"
  s.license      = { :type => "Apache-2.0", :text => File.read(File.expand_path("../../LICENSE", __dir__)) }
  s.authors      = { "TTDO contributors" => "https://github.com/alexeyshmelev/TTDO" }
  s.ios.deployment_target = '14.0'
  s.osx.deployment_target = '11.0'
  s.source       = { :http => "file://#{File.expand_path('Framework', __dir__)}" }

  s.vendored_frameworks = ["Framework/TrustTunnelClient.xcframework", "Framework/VpnClientFramework.xcframework"]
end
