pub mod data;
pub mod error;

use data::{
  Bag, Generator, GeneratorAmountRange, GeneratorType, Info, InstrumentHeader, Modulator, PresetHeader, SFData, SampleData,
  SampleHeader,
};

use crate::error::ParseError;
use std::io::{Read, Seek};

#[derive(Debug)]
pub struct Preset
{
  pub header : PresetHeader,
  pub zones : Vec<Zone>,
}

#[derive(Debug)]
pub struct Instrument
{
  pub header : InstrumentHeader,
  pub zones : Vec<Zone>,
}

#[derive(Debug)]
pub struct SoundFont2
{
  pub info : Info,
  pub presets : Vec<Preset>,
  pub instruments : Vec<Instrument>,
  pub sample_headers : Vec<SampleHeader>,
  pub sample_data : SampleData,
}

impl SoundFont2
{
  pub fn load<F : Read + Seek>(file : &mut F) -> Result<Self, ParseError> { SFData::load(file).map(Self::from_data) }

  pub fn from_data(data : SFData) -> Self
  {
    fn get_zones(zones : &[Bag], modulators : &[Modulator], generators : &[Generator], start : usize, end : usize) -> Vec<Zone>
    {
      let mut zone_items = Vec::new();
      for j in start .. end
      {
        let curr = zones.get(j).unwrap();
        let next = zones.get(j + 1);

        let mod_list = {
          let start = curr.modulator_id as usize;
          let end = if let Some(next) = next
          {
            next.modulator_id as usize
          }
          else
          {
            zones.len()
          };

          let mut list = Vec::new();

          for i in start .. end
          {
            let item = modulators.get(i);
            if let Some(item) = item
            {
              list.push(item.to_owned());
            }
          }
          list
        };

        let gen_list = {
          let start = curr.generator_id as usize;
          let end = if let Some(next) = next
          {
            next.generator_id as usize
          }
          else
          {
            zones.len()
          };

          let mut list = Vec::new();

          for i in start .. end
          {
            let item = generators.get(i);
            if let Some(item) = item
            {
              list.push(item.to_owned());
            }
          }
          list
        };

        zone_items.push(Zone { mod_list, gen_list });
      }
      zone_items
    }

    let instruments = {
      let headers = &data.hydra.instrument_headers;
      let zones = &data.hydra.instrument_bags;
      let modulators = &data.hydra.instrument_modulators;
      let generators = &data.hydra.instrument_generators;

      let iter = headers.iter();
      let mut iter_peek = headers.iter();
      // `iter_peek` has to be one item ahead of `iter`
      iter_peek.next();

      let mut list = Vec::new();

      for header in iter
      {
        let curr = header;
        let next = iter_peek.next();

        let start = curr.bag_id as usize;

        let end = if let Some(next) = next
        {
          next.bag_id as usize
        }
        else
        {
          zones.len()
        };

        let zone_items = get_zones(&zones, &modulators, &generators, start, end);

        // Ignore Terminator
        if header.name != "EOS"
        {
          list.push(Instrument {
            header : header.clone(),
            zones : zone_items,
          })
        }
      }
      list
    };

    let presets = {
      let headers = &data.hydra.preset_headers;
      let zones = &data.hydra.preset_bags;
      let modulators = &data.hydra.preset_modulators;
      let generators = &data.hydra.preset_generators;

      let iter = headers.iter();
      let mut iter_peek = headers.iter();
      // `iter_peek` has to be one item ahead of `iter`
      iter_peek.next();

      let mut list = Vec::new();
      for header in iter
      {
        let curr = header;
        let next = iter_peek.next();

        let start = curr.bag_id as usize;

        let end = if let Some(next) = next
        {
          next.bag_id as usize
        }
        else
        {
          zones.len()
        };

        let zone_items = get_zones(&zones, &modulators, &generators, start, end);

        // Ignore Terminator
        if header.name != "EOP"
        {
          list.push(Preset {
            header : header.clone(),
            zones : zone_items,
          })
        }
      }

      list
    };

    Self {
      info : data.info,
      presets,
      instruments,
      sample_headers : data
                .hydra
                .sample_headers
                .into_iter()
                // Ignore Terminator
                .filter(|h| h.name != "EOS")
                .collect(),
      sample_data : data.sample_data,
    }
  }

  pub fn sort_presets(mut self) -> Self
  {
    self.presets.sort_by(|a, b| {
      let aval = (a.header.bank as i32) << 16 | a.header.preset as i32;
      let bbal = (b.header.bank as i32) << 16 | b.header.preset as i32;
      let cmp = aval - bbal;

      if cmp < 0
      {
        std::cmp::Ordering::Less
      }
      else if cmp > 0
      {
        std::cmp::Ordering::Greater
      }
      else
      {
        std::cmp::Ordering::Equal
      }
    });
    self
  }
}

#[derive(Debug, Clone)]
pub struct Zone
{
  pub mod_list : Vec<Modulator>,
  pub gen_list : Vec<Generator>,
}

impl Zone
{
  pub fn key_range(&self) -> Option<&i16>
  {
    self
      .gen_list
      .iter()
      .find(|g| g.ty == GeneratorType::KeyRange)
      .map(|g| g.amount.as_i16().unwrap())
  }

  pub fn vel_range(&self) -> Option<&GeneratorAmountRange>
  {
    self
      .gen_list
      .iter()
      .find(|g| g.ty == GeneratorType::VelRange)
      .map(|g| g.amount.as_range().unwrap())
  }

  pub fn instrument(&self) -> Option<&u16>
  {
    self
      .gen_list
      .iter()
      .find(|g| g.ty == GeneratorType::Instrument)
      .map(|g| g.amount.as_u16().unwrap())
  }

  pub fn sample(&self) -> Option<&u16>
  {
    self
      .gen_list
      .iter()
      .find(|g| g.ty == GeneratorType::SampleID)
      .map(|g| g.amount.as_u16().unwrap())
  }
}
