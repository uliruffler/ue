This project is an editor that aims to provide typical text editing features
as known from visual editors like Notepad++ but runs in the terminal. I don't
plan to provide APIs for external use, so there is no need for public
functions, structs, or modules.

When changing code, always check that tests still pass and the compiler shows
no warnings (not in build and not in test). If you add new features, please
also add tests for them.