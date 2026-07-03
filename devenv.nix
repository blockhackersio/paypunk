{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: {
  languages.rust.enable = true;

  packages = with pkgs; [ bc ];

  enterShell = ''
    alias setup_test_wallet='${pkgs.bash}/bin/bash scripts/setup-test-wallet.sh'
    alias start_ethereum='${pkgs.bash}/bin/bash scripts/start-ethereum.sh'
    alias start_zcash='${pkgs.bash}/bin/bash scripts/start-zcash.sh'
    alias get_balance='${pkgs.bash}/bin/bash scripts/get-balance.sh'
    alias kp='${pkgs.bash}/bin/bash scripts/key-daemon.sh'
    alias pp='${pkgs.bash}/bin/bash scripts/wallet-daemon.sh'
    alias tui='${pkgs.bash}/bin/bash scripts/ui.sh'
    alias cb='cargo build'
    alias ct='cargo test'
  '';
}
