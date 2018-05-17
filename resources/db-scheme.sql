DROP TABLE IF EXISTS DBNAME.`accounts`;
CREATE TABLE IF NOT EXISTS DBNAME.`accounts` (
  `id` varchar(64) NOT NULL,
  `username` varchar(250) NOT NULL,
  `password` varchar(64) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf32;

DROP TABLE IF EXISTS DBNAME.`files`;
CREATE TABLE IF NOT EXISTS DBNAME.`files` (
`id` bigint(20) NOT NULL,
  `device_id` varchar(100) NOT NULL,
  `original_name` varchar(10000) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf32;

DROP TABLE IF EXISTS DBNAME.`files_versions`;
CREATE TABLE IF NOT EXISTS DBNAME.`files_versions` (
`id` bigint(20) NOT NULL,
  `file_id` int(11) NOT NULL,
  `created` datetime NOT NULL,
  `size` int(11) NOT NULL,
  `hash` char(64) NOT NULL,
  `storage_name` char(64) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf32;

DROP TABLE IF EXISTS DBNAME.`sessions`;
CREATE TABLE IF NOT EXISTS DBNAME.`sessions` (
  `id` varchar(64) NOT NULL,
  `created` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `last_used` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `device_id` varchar(200) NOT NULL,
  `account_id` varchar(64) NOT NULL,
  `pass` varchar(200) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf32;


ALTER TABLE DBNAME.`accounts`
 ADD PRIMARY KEY (`id`);

ALTER TABLE DBNAME.`files`
 ADD PRIMARY KEY (`id`), MODIFY `id` bigint(20) NOT NULL AUTO_INCREMENT;

ALTER TABLE DBNAME.`files_versions`
 ADD PRIMARY KEY (`id`), MODIFY `id` bigint(20) NOT NULL AUTO_INCREMENT, ADD UNIQUE KEY `storage_name_unique` (`storage_name`);

ALTER TABLE DBNAME.`sessions`
 ADD PRIMARY KEY (`id`);
