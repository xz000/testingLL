using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillR : MonoBehaviour
{
    public Toggle R1;
    public Toggle R1a;
    public Toggle R1b;
    public Toggle R2;
    public Toggle R2a;
    public Toggle R2b;
    public Toggle R3;
    public Toggle R3a;
    public Toggle R3b;
    public Image IconR;
    GameObject Soldier;

    public void SetR()
    {
        AllROff();
        IconR.GetComponent<CooldownImage>().enabled = true;
        if (R1.isOn && R1a.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillR1>().CalcFA;
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR1;
            return;
        }
        if (R1.isOn && R1b.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillR1b>().CalcFA;
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR1b;
            return;
        }
        if (R2.isOn && R2a.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillR2>().CalcFA;
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR2;
            return;
        }
        if (R2.isOn && R2b.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillR2b>().CalcFA;
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR2b;
            return;
        }
        if (R3.isOn && R3a.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<TestSkill02>().CalcFA;
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.TestSkill02;
            return;
        }
        if (R3.isOn && R3b.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconR.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillR3b>().CalcFA;
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR3b;
            return;
        }
    }

    public void AllROff()
    {
        IconR.GetComponent<CooldownImage>().enabled = false;
        GetComponent<SkillsLink>().KeyRSkill = null;
    }
}
