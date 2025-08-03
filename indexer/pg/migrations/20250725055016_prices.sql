-- Add migration script here

CREATE TYPE resolution AS ENUM
('S1', 'M1', 'M5', 'M15', 'H1', 'D1');

CREATE TABLE token (
    mint VARCHAR PRIMARY KEY NOT NULL,
    name VARCHAR,
    symbol VARCHAR,
    uri VARCHAR
);

CREATE TABLE trades
(
    datetime TIMESTAMP NOT NULL,
    mint_acc VARCHAR NOT NULL,
    resol resolution NOT NULL,
    open_price FLOAT8 NOT NULL,
    close_price FLOAT8 NOT NULL,
    high_price FLOAT8 NOT NULL,
    low_price FLOAT8 NOT NULL,
    volume FLOAT8 NOT NULL,

    CONSTRAINT trades_pk PRIMARY KEY (datetime, mint_acc, resol),
    CONSTRAINT trades_token_fk FOREIGN KEY (mint_acc) REFERENCES token (mint)
);