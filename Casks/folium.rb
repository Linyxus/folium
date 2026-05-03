cask "folium" do
  version "0.1.4"
  sha256 "44d3835f327d9dd5478768de19a3ad3233c86d187ffa8837446455b62859a0a6"

  url "https://github.com/Linyxus/folium/releases/download/v#{version}/Folium.dmg"
  name "Folium"
  desc "Native macOS PDF reader"
  homepage "https://github.com/Linyxus/folium"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "Folium.app"

  # The app isn't notarized; strip the download quarantine so first launch
  # doesn't trip Gatekeeper's "unidentified developer" dialog.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Folium.app"]
  end

  zap trash: [
    "~/Library/Preferences/com.linyxus.folium.plist",
    "~/Library/Saved Application State/com.linyxus.folium.savedState",
    "~/Library/Caches/com.linyxus.folium",
    "~/Library/HTTPStorages/com.linyxus.folium",
  ]
end
