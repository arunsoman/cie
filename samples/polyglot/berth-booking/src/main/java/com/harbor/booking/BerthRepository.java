package com.harbor.booking;

import org.springframework.data.jpa.repository.JpaRepository;

public interface BerthRepository extends JpaRepository<Berth, Long> {
  Berth findByBerthCode(String berthCode);
}
