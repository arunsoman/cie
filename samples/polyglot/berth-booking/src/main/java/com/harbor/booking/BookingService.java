package com.harbor.booking;

import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class BookingService {
  private final BerthRepository berthRepository;

  public BookingService(BerthRepository berthRepository) {
    this.berthRepository = berthRepository;
  }

  @Transactional
  public Berth occupy(String owner) {
    Berth berth = berthRepository.findByBerthCode(owner);
    if (berth == null) {
      throw new IllegalArgumentException("berth not found " + owner);
    }
    if (!berth.isAvailable()) {
      throw new IllegalStateException("berth already occupied " + owner);
    }
    berth.setOccupied(true);
    return berthRepository.save(berth);
  }
}
