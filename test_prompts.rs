static mut INPUT_1: i32 = 0;
static mut OUTPUT_1: i32 = 0;

fn main() {
    println!(
        "{}",
        llminate::ai::git_prompts::get_git_history_prompt().len()
    );
    println!(
        "{}",
        llminate::ai::git_prompts::get_claude_md_prompt().len()
    );
}

impl WasmModule {
    // / returns 0 if the output matches
    // ...
    fn func_4(&mut self) -> Option<i32> {
        v0 = TaggedVal::from(unsafe { INPUT_1 });
        v0 = TaggedVal::from(unsafe { OUTPUT_1 });
        // / equivalence - checking harness .
        fn equvalence() {
            bolero::check!().for_each(|(input)| {
                let llm_fn_output = llm_generated_reverse();
                unsafe {
                    INPUT_1 = input;
                    OUTPUT_1 = llm_fn_output;
                    let mut wasm_module = WasmModule::new();
                    wasm_module._start().unwrap();
                    assert!(wasm_module.func_4().unwrap() == 0);
                }
            })
        }
    }
}
