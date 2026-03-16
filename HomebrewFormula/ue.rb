class Ue < Formula
  desc "Simple terminal text editor with configurable key bindings"
  homepage "https://github.com/uliruffler/ue"
  url "https://github.com/uliruffler/ue/archive/refs/tags/v0.0.1.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
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
