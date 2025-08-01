-- Add migration script here

CREATE TABLE prices
(
    datetime TIMESTAMP PRIMARY KEY NOT NULL,
    price DOUBLE PRECISION NOT NULL
);
