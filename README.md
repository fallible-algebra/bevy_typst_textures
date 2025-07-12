# bevy_typst_textures

A simple resource for getting `Handle<Image>` out of structured, zipped typst projects.

This is very limited at this time, as it's mostly developed around my own needs.

## Expected structure for Typst Assets

A **`.zip`** file containing:
1. a **`main.typ`** file.
2. a **`package.toml`** file:
    - This doesn't need to be populated with anything right now.
    - That said, it expects:
        - a name field
        - an list of author strings
        - a list of bevy `asset/` folder asset requests (doesn't do anything right now)
        - a list of typst "universe" package requests (doesn't do anything right now)
3. Inclusion of all fonts needed (they can exist anywhere, but a `fonts/` folder is a good idea)
4. Whatever assets needed, reference them in a typst project like you would in any other typst project.