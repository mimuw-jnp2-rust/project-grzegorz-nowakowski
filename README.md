# Sendino

## Autorzy
- Grzegorz Nowakowski
- izmael7

## Opis
Projekt aplikacji chatu napisanej w języku Rust, oparty o model komunikacji
`client-server`. Udostępnia jeden pokój do komunikacji dla wielu użytkowników.

## Funkcjonalności
Serwer obsługuje wielu klientów jednocześnie. Po otrzymaniu wiadomości
od klienta odpowiednią ją formatuje i rozsyła grupowo do pozostałych
użytkowników. Wiadomości są przekazywane w formacie JSON.

## Biblioteki
- tokio
- serde_json
- crossterm

## Sposób użycia
Uruchamianie serwera:
`cargo run --bin jnp-chat-server <host>:<port>`

Uruchamianie klienta:
`cargo run --bin jnp-chat-client <host>:<port> <name>`