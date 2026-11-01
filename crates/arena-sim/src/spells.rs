use crate::specs::Specialisation;

#[derive(Clone, Copy, Debug)]
pub struct Sort {
    pub nom: &'static str,
    pub specialisation: Specialisation,
    pub degats: u16,
    pub soins: u16,
    pub cout_ressource: u16,
    pub cout_vie: u16,
    pub cooldown_ticks: u32,
    pub portee: u16,
}