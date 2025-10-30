# Blutgang NixOS Module

The following is a short guide on how to use the nixos modules for testing and
using blutgang in production.

## Building a VM for Testing

The flake attribute `checks.<system>.metrics-vm` has some extra virtualisation settings
as well as user settings. It forwards ports for SSH and grafana, and creates a
blutgang linux group, as well as adds maintainer keys to a blutgang user.
The blutgang user does not need a password for sudo because it is a part of
the `wheel` group, and its password is empty.

To build and run a VM from a configuration in the terminal:
```sh
nix run .#checks.<system>.metrics-vm -- -nographic
```

Once running, login either with the username `blutgang`, or via SSH:
```sh
ssh blutgang@localhost -p 2222
```

### Viewing Metrics from the VM

The grafana dashboard is forwarded on port `2342`, and can be viewed in the browser
at `localhost:2342` with username and password, `admin`. It will ask you to update
the login information for security reasons, but this is an ephemeral VM so you can skip it.
There is a `dashboard` field in the sidebar to the left to view metrics.

The same functionality can be achieved for end-user configurations, but we recommend
reading the relevant docs and configuring for your specific use case. Some settings
that are relevant to debugging in a VM are hazardous to production environments. For
that reason, we don't include these settings as modules in the flake outputs.

## Building for Production

### Building a Docker Image

For single user docker environments, like kubernetes deployments may use, there is
a `packages.<system>.dockerImage` that is built without the `journald` feature.
This is a minimal docker image containing only the relevant packages for running
blutgang.

Other virtualisation options can be found at [search.nixos.org/options](https://search.nixos.org/options?channel=unstable&query=virtualisation).

### Building a Bootable Image

In some cases you may just need a single shared server, in which case the default
module from the flake only needs the disk partition. This can be configured
easily with tools such as [`disko`](https://github.com/nix-community/disko).

Add the relevant modules and configure the partition layout:
```nix
{
  description = "An example blutgang nixos configuration";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    blutgang.url = "github:rainshowerLabs/blutgang";
  };
  outputs = { self, ... }@inputs:
    {
      nixosConfigurations.default = inputs.nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        modules = [
          ./your_partition_layout.nix
          inputs.blutgang.nixosModules.default
          {
            services.blutgang = {
              enable = true;
              settings.rpc = [{
                url = "https://eth.merkle.io";
                max_consecutive = 150;
                max_per_second = 200;
              }];
            };
          }
        ];
      };
    };
}
```

Then build the toplevel configuration:
```sh
nix build .#nixosConfigurations.default.config.system.build.toplevel -L
```

Depending on the server provider, it may also be helpful to have a look at [nixos-anywhere](https://github.com/nix-community/nixos-anywhere)
, as well as the available [virtualisation options](https://search.nixos.org/options?channel=unstable&query=virtualisation).
