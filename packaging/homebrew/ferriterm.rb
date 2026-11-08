cask "ferriterm" do
  version "0.1.0"

  on_arm do
    sha256 "REPLACE_WITH_AARCH64_DMG_SHA256"
    url "https://github.com/chensnails/ferriterm/releases/download/v#{version}/Ferriterm_#{version}_aarch64.dmg"
  end

  on_intel do
    sha256 "REPLACE_WITH_X64_DMG_SHA256"
    url "https://github.com/chensnails/ferriterm/releases/download/v#{version}/Ferriterm_#{version}_x64.dmg"
  end

  name "Ferriterm"
  desc "Cross-platform SSH terminal client"
  homepage "https://github.com/chensnails/ferriterm"

  app "Ferriterm.app"
end
