/*
 * iNilla: the executable's entry point. Everything else is Rust (crates/inilla-ios), linked in
 * from libinilla_ios.a; winit's runner inside it calls UIApplicationMain and never returns.
 */
extern int inilla_main(void);

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    return inilla_main();
}
