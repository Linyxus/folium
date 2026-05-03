cask "folium" do
  version "0.1.3"
  sha256 "bb310b57bcf278e13273b76ab06561d5f0cc999385dceeb9fd9e74a82a497a95"

  url "https://github.com/Linyxus/folium/releases/download/v#{version}/Folium.dmg"
  name "Folium"
  desc "Native macOS PDF reader"
  homepage "https://github.com/Linyxus/folium"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "Folium.app"

  zap trash: [
    "~/Library/Preferences/com.linyxus.folium.plist",
    "~/Library/Saved Application State/com.linyxus.folium.savedState",
    "~/Library/Caches/com.linyxus.folium",
    "~/Library/HTTPStorages/com.linyxus.folium",
  ]
end
