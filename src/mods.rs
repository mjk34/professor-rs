//!---------------------------------------------------------------------!
//! This file contains a collection of MODERATOR related commands to    !
//! to better serve the facilitation of professorBot                    !
//!                                                                     !
//! Commands:                                                           !
//!     [ ] - give_creds                                                !
//!     [ ] - take_creds                                                !
//!     [ ] - give_wishes                                               !
//!     [ ] - refund_tickets                                            !
//!---------------------------------------------------------------------!

use std::collections::HashMap;

use std::sync::Arc;
use std::time::Duration;

use chrono::prelude::Utc;
use poise::serenity_prelude::futures::StreamExt;

use poise::serenity_prelude::{EditMessage, ReactionType, UserId};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use tokio::sync::RwLock;

use crate::data::{self, VoiceUser};
use crate::serenity;
use crate::{Context, Error};
use serenity::Color;
