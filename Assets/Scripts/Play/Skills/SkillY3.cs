using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillY3 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    //public float maxdistance;
    public float maxtime = 4;
    public float bulletspeed = 4;
    public GameObject fireball;
    public float force = 1.5f;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillY3()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        GetComponent<DoSkill>().BeforeSkill();
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = (actionplace - singplace).normalized();
        DoFire(singplace + skilldirection / (Fix64)2, skilldirection * (Fix64)bulletspeed);
        currentcooldown = 0;
        skillavaliable = false;
    }

    void DoFire(Fix64Vector2 fireplace, Fix64Vector2 speed2d)
    {
        GameObject bullet;
        bullet = Instantiate(fireball, fireplace.ToV2(), Quaternion.identity);
        bullet.GetComponent<CentrallyConstentField>().sender = gameObject;
        bullet.GetComponent<CentrallyConstentField>().setspeed(force);
        bullet.GetComponent<CountdownScript>().maxtime = maxtime;
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d.ToV2();
    }

    void SkillY3SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }

    void LinkToIcon()
    {
        if (enabled)
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iY.Fif = CalcFA;
    }
}
