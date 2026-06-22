cask "folium" do
  version "0.1.8"
  sha256 "71cf768991bc53e6687d92550ee1dec7233b2b5f43160e6db9290db98a609544"

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
