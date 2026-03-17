class Ue < Formula
  desc "Simple terminal text editor with configurable key bindings"
  homepage "https://github.com/uliruffler/ue"
  url "https://github.com/uliruffler/ue/archive/refs/tags/v0.1.1.tar.gz"
  sha256 "4f97eaf268b75e75436f713e9250077e6cb3fe02115194bbfd15b009b52fb3e3"
  license "GPL-3.0-only"
  head "https://github.com/uliruffler/ue.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ue --version")
  end
end
