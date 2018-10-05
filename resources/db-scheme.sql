SET FOREIGN_KEY_CHECKS = 0; 

DROP TABLE IF EXISTS `DBNAME`.`accounts`;
CREATE TABLE IF NOT EXISTS `DBNAME`.`accounts` (
  `id` varchar(64) NOT NULL PRIMARY KEY,
  `username` varchar(250) NOT NULL,
  `password` varchar(64) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf32;

DROP TABLE IF EXISTS `DBNAME`.`files`;
CREATE TABLE IF NOT EXISTS `DBNAME`.`files` (
`id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
  `device_id` varchar(100) NOT NULL,
  `original_name` varchar(10000) NOT NULL,
  `identity_hash` varchar(64) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf32;

DROP TABLE IF EXISTS `DBNAME`.`files_versions`;
CREATE TABLE IF NOT EXISTS `DBNAME`.`files_versions` (
`id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
  `file_id` bigint(20) NOT NULL,
  `created` datetime NOT NULL,
  `size` int(11) NOT NULL,
  `hash` char(64) NOT NULL,
  `storage_name` char(64) NOT NULL,
  FOREIGN KEY (file_id)
        REFERENCES `DBNAME`.`files` (id)
        ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf32;

DROP TABLE IF EXISTS `DBNAME`.`sessions`;
CREATE TABLE IF NOT EXISTS `DBNAME`.`sessions` (
  `id` varchar(64) NOT NULL PRIMARY KEY,
  `created` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `last_used` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `device_id` varchar(200) NOT NULL,
  `account_id` varchar(64) NOT NULL,
  `pass` varchar(200) NOT NULL,
  FOREIGN KEY (account_id)
        REFERENCES `DBNAME`.`accounts` (id)
        ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf32;


ALTER TABLE `DBNAME`.`files`
  ADD UNIQUE KEY `identity_hash` (`identity_hash`);

ALTER TABLE `DBNAME`.`files_versions`
  ADD UNIQUE KEY `storage_name_unique` (`storage_name`);

SET FOREIGN_KEY_CHECKS = 1;