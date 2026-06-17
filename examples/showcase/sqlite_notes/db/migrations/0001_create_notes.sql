create table notes (
  id integer primary key,
  title text not null,
  body text not null,
  status text not null
);

insert into notes (title, body, status) values
  ('Welcome to Ricochet', 'This note is served from SQLite through Active Record.', 'open'),
  ('Beta target', 'The app is meant for developer testing, not production deployment.', 'open');
