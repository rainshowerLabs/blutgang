# Add authorized keys to openssh. Github keys can be found at `github.com/<username>.keys`.

{ lib, ... }:
let
  inherit (builtins) readFile fetchurl;
  inherit (lib) concatLists splitString mapAttrsToList;

  users = {
    # Add additional github usernames to include SSH keys for testing in VMs
    makemake-kbo = "sha256:0vykqzbhnq5yjglr4h6pr5x6hwjqxgdwmml8slqg4ddgrbiksq65";
    eureka-cpu = "sha256:00nrgq1difw7zkzzxbi9kw51pqc3757zq3kch8qlwwq0j84zlh5f";
  };

  collectGitHubSSHKeys = users:
    concatLists (mapAttrsToList
      (user: sha256:
        splitString "\n" (readFile (fetchurl {
          inherit sha256;
          url = "https://github.com/${user}.keys";
        }))
      )
      users);
in
{
  security.sudo.wheelNeedsPassword = false;
  users.groups.blutgang = { };
  users.users.blutgang = {
    description = "blutgang";
    isNormalUser = true;
    password = "";
    home = "/home/blutgang";
    group = "blutgang";
    extraGroups = [ "wheel" ];
    openssh.authorizedKeys.keys = collectGitHubSSHKeys users;
  };
}
