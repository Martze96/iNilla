//! The guild tabard designer's engine surface (decision 1977): the `TabardModel` kind's own method
//! table (`0x84ee40`, ten verbs — filled from wow-re `system/ui/scratch/tabard-designer.md`) and
//! the window's two globals.
//!
//! `GetTabardCreationCost()` is a **hard-coded constant**: `0x6d6de0` is `mov eax,0x186a0; ret` —
//! 100 000 copper, ten gold, no `.data` cell and no server input (wow-re
//! `system/ui/scratch/petition-charter-api.md`, the adjacent-family paragraph; VERIFIED there).

use mlua::Lua;

/// Registry key of the TabardModel method table — probed before PlayerModel's and Model's
/// (`object.rs`'s kind chain).
pub(super) const REG_TABARDMODEL_METHODS: &str = "__benilla_tabardmodel_methods";

/// `0x6d6de0`: the designer's price, in copper.
pub const TABARD_CREATION_COST: u32 = 100_000;

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let m = lua.create_table()?;
    lua.set_named_registry_value(REG_TABARDMODEL_METHODS, m)?;

    let g = lua.globals();
    // `GetTabardCreationCost()` — 0 args, one number, the constant.
    g.set(
        "GetTabardCreationCost",
        lua.create_function(|_, ()| Ok(i64::from(TABARD_CREATION_COST)))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;

    #[test]
    fn the_creation_cost_is_ten_gold_and_a_tabard_model_is_its_own_kind() {
        let s = UiScript::new().unwrap();
        assert_eq!(
            s.eval::<i64>("return GetTabardCreationCost()").unwrap(),
            100_000
        );
        s.run(r#"t = CreateFrame("TabardModel", "TM")"#).unwrap();
        assert_eq!(
            s.eval::<String>("return t:GetObjectType()").unwrap(),
            "TabardModel"
        );
        assert!(s
            .eval::<bool>("return t:IsObjectType('PlayerModel') and t:IsObjectType('Model')")
            .unwrap());
        assert!(
            s.eval::<bool>("return t.SetUnit ~= nil and t.SetRotation ~= nil")
                .unwrap(),
            "the inherited pane verbs"
        );
    }
}
