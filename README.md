# bevy_typst_textures

A simple `Resource` for generating rasterised textures (`Handle<Image>`) out of structured, zipped typst projects, built on `typst-as-lib`.

This is very limited at this time, as it's mostly developed around my own needs.

## Example

## Expected structure for Typst Assets

A **`.zip`** file containing, at the very least:
1. a **`main.typ`** file.
2. a **`package.toml`** file:
    - This doesn't need to be populated with anything right now.
    - That said, it expects:
        - a name field
        - an list of author strings
        - a list of bevy `asset/` folder asset requests (doesn't do anything right now)
        - a list of typst "universe" package requests (doesn't do anything right now)
    - All files and fonts referenced by the 
3. Inclusion of all fonts needed (they can exist anywhere, but a `fonts/` folder is a good idea)
4. Whatever assets needed, reference them in a typst project like you would in any other typst project.

## Limitations

This project is built on top of the `typst-as-lib` crate, which provides a nice wrapper over the internals of `typst` for standalone projects. The limitations of `typst-as-lib` are inherited by this crate.

This package expects typst assets as zip archives to simplify the asset-fetching process (as outlined above).

Packages are supported, but not on web. This may change in the future, but for now this does not work.

The archive unzipping is a bit fragile right now, too. Lots of `unwrap`s and assumptions about how different OSs handle zip archives, and some ad-hoc dealing with how they pollute filesystems with metadata.

`add_job_with_data` uses serde to serialize the input data type to json before then de-seralizing it to typst's `Dict` type. This presents the regular `serde` overhead, mostly.

## Cargo Features

All these features are passthroughs to `typst-as-lib` features, except for `packages` which is both a passthrough *and* enables package fetching for all typst templates you load. This should be a plugin option, but for now it is not.

- ``

## Why not [`Velyst`](https://github.com/voxell-tech/velyst)?

This crate sits in the niche of needing rasterised textures rather than full & interactive typst integration.