package com.harbor.booking;

import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;

@RestController
@RequestMapping("/berths")
public class BerthController {
  private final BookingService bookingService;

  public BerthController(BookingService bookingService) {
    this.bookingService = bookingService;
  }

  @PostMapping("/{code}/book")
  public Berth book(@PathVariable String code) {
    return bookingService.occupy(code);
  }

  @GetMapping("/{code}")
  public Berth status(@PathVariable String code) {
    return bookingService.occupy(code);
  }
}
