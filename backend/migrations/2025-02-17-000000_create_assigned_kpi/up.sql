CREATE TABLE assigned_kpi
(
    id                        SERIAL PRIMARY KEY,
    mission_id                INTEGER          NOT NULL REFERENCES mission,
    player_id                 SMALLINT         NOT NULL REFERENCES player,
    target_kpi_component      SMALLINT         NOT NULL,
    kpi_component_delta_value DOUBLE PRECISION NOT NULL,
    total_delta_value         DOUBLE PRECISION NOT NULL,
    note                      TEXT
);