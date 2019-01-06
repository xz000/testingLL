using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillF : MonoBehaviour
{
    public Toggle F1;
    public Image IconF;
    GameObject Soldier;

    public void SetF()
    {
        if (Soldier == null)
            Soldier = gameObject.GetComponent<SkillsLink>().mySoldier;
        AllFOff();
        if (F1.isOn)
        {
            Soldier.GetComponent<SelfExplodeScript>().MyImageScript = IconF.GetComponent<CooldownImage>();
            Soldier.GetComponent<TestSkill03>().enabled = true;
            Soldier.GetComponent<SelfExplodeScript>().enabled = true;
            return;
        }
    }

    void AllFOff()
    {
        IconF.GetComponent<CooldownImage>().IconFillAmount = 1;
        Soldier.GetComponent<TestSkill03>().enabled = false;
        Soldier.GetComponent<SelfExplodeScript>().enabled = false;
        Soldier.GetComponent<SelfExplodeScript>().MyImageScript = null;
    }
}
