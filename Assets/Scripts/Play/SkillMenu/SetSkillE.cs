using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillE : MonoBehaviour
{
    public Toggle E1;
    public Toggle E1a;
    public Toggle E1b;
    public Toggle E2;
    public Toggle E2a;
    public Toggle E2b;
    public Toggle E3;
    public Toggle E3a;
    public Toggle E3b;
    public Toggle E4;
    public Toggle E4a;
    public Toggle E4b;
    public Image IconE;
    GameObject Soldier;

    public void SetE()
    {
        AllEOff();
        IconE.GetComponent<CooldownImage>().enabled = true;
        if (E1.isOn && E1a.isOn)
        {
            IconE.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillE1>().CalcFA;
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE1;
            return;
        }
        if (E1.isOn && E1b.isOn)
        {
            IconE.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillE1b>().CalcFA;
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE1b;
            return;
        }
        if (E2.isOn && E2a.isOn)
        {
            Soldier.GetComponent<SkillE2>().MyImageScript = IconE.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillE2>().enabled = true;
            return;
        }
        if (E2.isOn && E2b.isOn)
        {
            Soldier.GetComponent<SkillE2b>().MyImageScript = IconE.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillE2b>().enabled = true;
            return;
        }
        if (E3.isOn && E3a.isOn)
        {
            Soldier.GetComponent<SkillE3>().MyImageScript = IconE.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillE3>().enabled = true;
            return;
        }
        if (E3.isOn && E3b.isOn)
        {
            Soldier.GetComponent<SkillE3b>().MyImageScript = IconE.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillE3b>().enabled = true;
            return;
        }
    }

    public void AllEOff()
    {
        IconE.GetComponent<CooldownImage>().enabled = false;
        GetComponent<SkillsLink>().KeyESkill = null;
    }
}
