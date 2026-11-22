//! Ticket-Zustandsmaschine: Open -> Claimed -> Closed.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketState {
    Open,
    Claimed { by: u64 },
    Closed { by: u64 },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TicketError {
    #[error("Ticket ist bereits geschlossen")]
    AlreadyClosed,
    #[error("Ticket wurde bereits von <@{0}> uebernommen")]
    AlreadyClaimed(u64),
    #[error("Ticket ist nicht uebernommen")]
    NotClaimed,
}

impl TicketState {
    pub fn claim(&self, user: u64) -> Result<TicketState, TicketError> {
        match self {
            TicketState::Open => Ok(TicketState::Claimed { by: user }),
            TicketState::Claimed { by } => Err(TicketError::AlreadyClaimed(*by)),
            TicketState::Closed { .. } => Err(TicketError::AlreadyClosed),
        }
    }

    pub fn unclaim(&self) -> Result<TicketState, TicketError> {
        match self {
            TicketState::Claimed { .. } => Ok(TicketState::Open),
            TicketState::Open => Err(TicketError::NotClaimed),
            TicketState::Closed { .. } => Err(TicketError::AlreadyClosed),
        }
    }

    pub fn close(&self, user: u64) -> Result<TicketState, TicketError> {
        match self {
            TicketState::Closed { .. } => Err(TicketError::AlreadyClosed),
            _ => Ok(TicketState::Closed { by: user }),
        }
    }
}

/// Kanalname eines Tickets, z. B. "ticket-0042".
pub fn channel_name(number: u32) -> String {
    format!("ticket-{number:04}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() {
        let s = TicketState::Open.claim(1).unwrap();
        assert_eq!(s, TicketState::Claimed { by: 1 });
        assert_eq!(s.close(2).unwrap(), TicketState::Closed { by: 2 });
    }

    #[test]
    fn cannot_double_claim_or_reopen() {
        let s = TicketState::Open.claim(1).unwrap();
        assert_eq!(s.claim(2), Err(TicketError::AlreadyClaimed(1)));
        let c = s.close(1).unwrap();
        assert_eq!(c.claim(3), Err(TicketError::AlreadyClosed));
        assert_eq!(c.close(3), Err(TicketError::AlreadyClosed));
    }

    #[test]
    fn unclaim_requires_claim() {
        assert_eq!(TicketState::Open.unclaim(), Err(TicketError::NotClaimed));
    }

    #[test]
    fn names() {
        assert_eq!(channel_name(42), "ticket-0042");
    }
}
