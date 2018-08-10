# Debian Repository Builder

A simple utility for constructing and maintaining Debian repositories. Configuration of a repo is
based on the directory hierarchy, and a TOML configuration file. Real world repos to demonstrate
may be found bellow.

- [System76 CUDA Repo](https://github.com/system76/cuda)
- [Pop!\_OS Proprietary Repo](https://github.com/pop-os/repo-proprietary)


## Directory Structure

The root directory of a debrep-based repo will contain the following directories:

- **assets/**: where files that need to be linked at build-time are stored
  - **cache/**: files which debrep downloads from external sources, and should be cached between runs
  - **share/**: files that can be shared across packages, and are specified in the TOML config
  - **packages/**: files which are automatically linked to the build before building
- **build/**: debrep performs all builds within this directory.
  - Every file is linked / sourced here at build time.
  - After each successful build, files are moved into the repo.
- **debian/**: contains the debian configuration for each source package that needs one.
  - The directories within must have the same name as the source package they reference.
  - Each package directory contains the entire contents of the debian directory for that package.
- **metapackages/**: place your `metapackage.cfg` equivs files in here.
  - On build, they'll be generated and placed into the repo.
- **record/**: keeps tabs on what source packages have been built
- **repo/**: Contains the archive & associated dist and pool directories for each
- **sources.toml**: Configuration for the entire repo.

## Highly Parallel Distribution File Generation

Since this tool is written in Rust, one of the key focuses has been on making it do as much as it can in parallel,
as fast as it can do it. It uses thread pools, parallel iterators, and state machines to achieve that goal. Each
component of a suit; each architecture in those components; and each package in those architectures are all
processed in parallel. Data from each archive is also processed in parallel, and the final stage of processing
that data into information and writing it to various compressed archives is done in parallel as well. Our tool
should be fast with large archives.

## Source Building Support

Packages can be generated from sources so long as you provide the debian files necessary -- either by using existing
debian files in the upstream archive or git repository, or by providing your own through a variety of means.

## Components Support

Managing components are supported by this utility! There's currently a `default_component` variable for the config,
which will designate where packages will be stored by default. The `migrate` subcommand can be used to move packages
between components. After moving packages, the dist files will be re-generated.

## Contents Generation

Tools like `apt-file` require the the repository stores `Contents` archives, which it will download and read from
to find which packages contain what files in a repository. This tool will process and generate these files in parallel
as it is also processing the `Packages` archives.

### Repo Structure

This is what you can expect to see after a successful build. You may sync the dists and pool
directories to your package server to make your repository accessible to other machines.

```
repo/
  dists/
    cosmic/
    bionic/
      Contents-amd64
      Contents-amd64.gz
      Contents-amd64.xz
      InRelease
      proposed/
        binary-amd64/
          Packages
          Packages.gz
          Packages.xz
          Release
      main/
        binary-amd64/
          Packages
          Packages.gz
          Packages.xz
          Release
        source/
          Sources
          Sources.gz
          Sources.xz
        Release
        Release.gpg
  pool/
    cosmic/
    bionic/
      proposed/
        binary-amd64/
          p/
            package/
              package...
      main/
        binary-amd64/
          p/
            package/
              package_version_amd64.buildinfo
              package_version_amd64.changes
              package_version_amd64.deb
              package-dbgsym_version_amd64.ddeb
        source/
          p/
            package/
              package_version.dsc
              package_version.tar.xz
```

## Usage

### Create / update a Debian repository
```
debrep build [ -f | --force ]
debrep build packages <PACKAGES>... [ -f | --force ]
debrep build pool
debrep build dist
```

### Migrate packages between components
```
debrep migrate package1 package2 pacakge3 --from proposed --to main
```

### Clean up old packages
```
debrep clean
```

### Remove packages
```
debrep remove <PACKAGES>...
```

### Pretty-print the sources.toml configuration
```
debrep config
```

### Fetch a field from the configuration file
```
debrep config direct.atom-editor.version
```

### Update a field in the configuration file
```
deprep config direct.atom-editor.version ${NEW_VERSION}
deprep config direct.atom-editor.url ${NEW_URL}
```
