use actix_web::{web, App, HttpServer, HttpResponse, Result, middleware::Logger};
use actix_cors::Cors;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lottery {
    pub id: String,
    pub admin: String,
    pub ticket_price: u64,
    pub participants: Vec<Participant>,
    pub is_active: bool,
    pub prize_pool: u64,
    pub winner: Option<String>,
    pub created_at: String,
    pub end_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub wallet_address: String,
    pub tickets_bought: u32,
}

#[derive(Deserialize)]
pub struct CreateLotteryRequest {
    pub admin: String,
    pub ticket_price: u64,
    pub end_time: Option<String>,
}

#[derive(Deserialize)]
pub struct BuyTicketRequest {
    pub wallet_address: String,
    pub tickets: u32,
}

#[derive(Deserialize)]
pub struct PickWinnerRequest {
    pub admin: String,
}

type LotteryState = Arc<Mutex<HashMap<String, Lottery>>>;

async fn create_lottery(
    data: web::Json<CreateLotteryRequest>,
    state: web::Data<LotteryState>,
) -> Result<HttpResponse> {
    let lottery_id = Uuid::new_v4().to_string();
    let lottery = Lottery {
        id: lottery_id.clone(),
        admin: data.admin.clone(),
        ticket_price: data.ticket_price,
        participants: Vec::new(),
        is_active: true,
        prize_pool: 0,
        winner: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        end_time: data.end_time.clone(),
    };

    let mut lotteries = state.lock().unwrap();
    lotteries.insert(lottery_id.clone(), lottery.clone());
    
    Ok(HttpResponse::Ok().json(lottery))
}

async fn buy_ticket(
    lottery_id: web::Path<String>,
    data: web::Json<BuyTicketRequest>,
    state: web::Data<LotteryState>,
) -> Result<HttpResponse> {
    // Validate input
    if data.tickets == 0 {
        return Ok(HttpResponse::BadRequest().json("Must buy at least 1 ticket"));
    }
    if data.tickets > 10000 {
        return Ok(HttpResponse::BadRequest().json("Cannot buy more than 10,000 tickets at once"));
    }
    if data.wallet_address.is_empty() {
        return Ok(HttpResponse::BadRequest().json("Wallet address is required"));
    }
    
    let mut lotteries = state.lock().unwrap();
    
    if let Some(lottery) = lotteries.get_mut(lottery_id.as_str()) {
        if !lottery.is_active {
            return Ok(HttpResponse::BadRequest().json("Lottery is not active"));
        }
        
        let total_cost = lottery.ticket_price * data.tickets as u64;
        lottery.prize_pool += total_cost;
        
        // Check if participant already exists
        if let Some(participant) = lottery.participants.iter_mut()
            .find(|p| p.wallet_address == data.wallet_address) {
            participant.tickets_bought += data.tickets;
        } else {
            lottery.participants.push(Participant {
                wallet_address: data.wallet_address.clone(),
                tickets_bought: data.tickets,
            });
        }
        
        Ok(HttpResponse::Ok().json(lottery.clone()))
    } else {
        Ok(HttpResponse::NotFound().json("Lottery not found"))
    }
}

async fn pick_winner(
    lottery_id: web::Path<String>,
    admin_data: web::Json<PickWinnerRequest>,
    state: web::Data<LotteryState>,
) -> Result<HttpResponse> {
    let mut lotteries = state.lock().unwrap();
    
    if let Some(lottery) = lotteries.get_mut(lottery_id.as_str()) {
        // Check admin authorization
        if lottery.admin != admin_data.admin {
            return Ok(HttpResponse::Forbidden().json("Only the lottery admin can pick a winner"));
        }
        
        if !lottery.is_active {
            return Ok(HttpResponse::BadRequest().json("Lottery is already ended"));
        }
        
        if lottery.participants.is_empty() {
            return Ok(HttpResponse::BadRequest().json("No participants in lottery"));
        }
        
        // Calculate total tickets across all participants
        let total_tickets: u32 = lottery.participants.iter().map(|p| p.tickets_bought).sum();
        if total_tickets == 0 {
            return Ok(HttpResponse::BadRequest().json("No tickets sold"));
        }
        
        // Use cryptographically secure random selection
        use rand::rngs::OsRng;
        use rand::RngCore;
        let mut rng = OsRng;
        let winning_ticket_number = (rng.next_u32() % total_tickets) + 1;
        
        // Find the winner without creating a large vector
        let mut current_ticket = 0;
        for participant in &lottery.participants {
            current_ticket += participant.tickets_bought;
            if winning_ticket_number <= current_ticket {
                lottery.winner = Some(participant.wallet_address.clone());
                break;
            }
        }
        
        lottery.is_active = false;
        
        Ok(HttpResponse::Ok().json(lottery.clone()))
    } else {
        Ok(HttpResponse::NotFound().json("Lottery not found"))
    }
}

async fn get_lottery(
    lottery_id: web::Path<String>,
    state: web::Data<LotteryState>,
) -> Result<HttpResponse> {
    let lotteries = state.lock().unwrap();
    
    if let Some(lottery) = lotteries.get(lottery_id.as_str()) {
        Ok(HttpResponse::Ok().json(lottery))
    } else {
        Ok(HttpResponse::NotFound().json("Lottery not found"))
    }
}

async fn get_all_lotteries(state: web::Data<LotteryState>) -> Result<HttpResponse> {
    let lotteries = state.lock().unwrap();
    let lottery_list: Vec<Lottery> = lotteries.values().cloned().collect();
    Ok(HttpResponse::Ok().json(lottery_list))
}

async fn health_check() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json("Rondo Mundi Backend is running!"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    
    let lottery_state: LotteryState = Arc::new(Mutex::new(HashMap::new()));
    
    println!("🎲 Starting Rondo Mundi backend server on 0.0.0.0:8080");
    
    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();
            
        App::new()
            .app_data(web::Data::new(lottery_state.clone()))
            .wrap(cors)
            .wrap(Logger::default())
            .route("/", web::get().to(health_check))
            .route("/health", web::get().to(health_check))
            .route("/lottery", web::post().to(create_lottery))
            .route("/lottery/{lottery_id}", web::get().to(get_lottery))
            .route("/lottery/{lottery_id}/buy", web::post().to(buy_ticket))
            .route("/lottery/{lottery_id}/pick_winner", web::post().to(pick_winner))
            .route("/lotteries", web::get().to(get_all_lotteries))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
