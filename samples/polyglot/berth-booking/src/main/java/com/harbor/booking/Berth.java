package com.harbor.booking;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.GeneratedValue;

@Entity
public class Berth {
  @Id
  @GeneratedValue
  private Long id;

  private String berthCode;
  private int lengthMeters;
  private double pricePerNight;
  private boolean occupied;

  public boolean isAvailable() {
    return !occupied;
  }

  public Long getId() {
    return id;
  }
}
