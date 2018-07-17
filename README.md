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
- **record/**: keeps tabs on what source packages have been built
- **repo/**: Contains the archive & associated dist and pool directories for each
- **sources.toml**: Configuration for the entire repo.

### Repo Structure

This is what you can expect to see after a successful build. You may sync the dists and pool
directories to your package server to make your repository accessible to other machines.

```
repo/
  dists/
    bionic-proposed/
    bionic/
      InRelease
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
    bionic-proposed/
    bionic/
      main /
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
