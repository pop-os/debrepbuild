# Debian Repository Builder

This is a WIP project for automatically generating and maintaining Debian repositories from a TOML spec.

See the examples directory for an example of the current syntax for the project.

## Usage

### Create / update a Debian repository
```
debrepbuild
```

### Pretty-print the sources.toml configuration
```
debrepbuild config
```

### Fetch a field from the configuration file
```
debrepbuild config direct.atom-editor.version
```

### Update a field in the configuration file
```
deprepbuild config direct.atom-editor.version = ${NEW_VERSION}
deprepbuild config direct.atom-editor.url = ${NEW_URL}
```
