use orbit_domain::Skill;

const SKILLS_START: &str = "<!-- ORBIT SKILLS:START -->";
const SKILLS_END: &str = "<!-- ORBIT SKILLS:END -->";

pub(crate) fn compose_agents_md(existing: &str, enabled_skills: &[Skill], memory: &str) -> String {
    let content = if let Some(start) = existing.find(SKILLS_START) {
        if let Some(end) = existing[start..].find(SKILLS_END) {
            format!(
                "{}{}",
                &existing[..start],
                &existing[start + end + SKILLS_END.len()..]
            )
        } else {
            existing.to_owned()
        }
    } else {
        existing.to_owned()
    };

    let mut base = content.trim().to_owned();
    while base.contains("\n\n\n") {
        base = base.replace("\n\n\n", "\n\n");
    }
    let mut block = SKILLS_START.to_owned();
    if !enabled_skills.is_empty() {
        block.push_str(
            "\n# Skills\n\nYou have these always-on skills. Use any that apply to the task, and combine them when useful:",
        );
        for skill in enabled_skills {
            block.push_str(&format!("\n\n## {}\n{}", skill.name, skill.instruction));
        }
    }
    let memory = memory.trim();
    let memory = if memory.is_empty() {
        "_(empty — nothing saved yet)_"
    } else {
        memory
    };
    block.push_str(&format!(
        "\n# Memory\nYou keep a long-term memory file at `.orbit/MEMORY.md` in your workspace. When you learn something durable and worth remembering — decisions made, the user's preferences, how this project is set up, or facts you'll need again — record it there as short bullet points, and update or remove entries when they change. Never store secrets, passwords, or tokens. Read your memory before asking about things you may already know.\n\n## Current memory\n{memory}\n{SKILLS_END}"
    ));

    if base.is_empty() {
        format!("{block}\n")
    } else {
        format!("{base}\n\n{block}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn skill(name: &str, instruction: &str) -> Skill {
        Skill {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: name.into(),
            instruction: instruction.into(),
            enabled: true,
        }
    }

    #[test]
    fn empty_existing_with_skill_contains_managed_content() {
        let output = compose_agents_md("", &[skill("X", "Do X")], "");
        assert!(output.contains(SKILLS_START));
        assert!(output.contains("# Skills"));
        assert!(output.contains("## X"));
        assert!(output.contains("Do X"));
    }

    #[test]
    fn appends_block_after_existing_prose() {
        let output = compose_agents_md("# My project\n\nnotes\n", &[skill("X", "Do X")], "");
        assert!(output.starts_with("# My project\n\nnotes\n"));
        assert!(output.find(SKILLS_START).unwrap() > 0);
    }

    #[test]
    fn composing_again_is_idempotent_without_duplicate_markers() {
        let first = compose_agents_md("# My project\n", &[skill("X", "Do X")], "");
        let second = compose_agents_md(&first, &[skill("X", "Do X")], "");
        assert_eq!(first, second);
        assert_eq!(second.matches(SKILLS_START).count(), 1);
    }

    #[test]
    fn removes_existing_block_and_preserves_prose() {
        let existing = "# My project\n\n<!-- ORBIT SKILLS:START -->\n# Skills\n\n## X\nDo X\n<!-- ORBIT SKILLS:END -->\n\nnotes\n";
        let output = compose_agents_md(existing, &[], "");
        assert!(output.starts_with("# My project\n\nnotes\n"));
        assert!(output.contains(SKILLS_START));
    }

    #[test]
    fn no_skills_still_renders_memory_section() {
        let output = compose_agents_md("", &[], "");
        assert!(output.contains(SKILLS_START));
        assert!(output.contains("# Memory"));
        assert!(output.contains("_(empty — nothing saved yet)_"));
        assert!(!output.contains("# Skills"));
    }

    #[test]
    fn embeds_memory_content() {
        let output = compose_agents_md("", &[], "- likes tersen replies\n- project uses pnpm");
        assert!(output.contains("## Current memory\n- likes tersen replies\n- project uses pnpm"));
    }

    #[test]
    fn no_markers_and_no_skills_returns_managed_content() {
        let existing = "notes without a trailing newline";
        let output = compose_agents_md(existing, &[], "");
        assert!(output.starts_with(existing));
        assert!(output.contains(SKILLS_START));
    }
}
